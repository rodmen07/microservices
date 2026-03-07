use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::validate_authorization_header,
    models::{ApiError, CreateReportRequest, DashboardSummary, SavedReport, UpdateReportRequest},
};

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = Json(ApiError {
        code: code.to_string(),
        message: message.to_string(),
        details: None,
    });
    (status, body).into_response()
}

fn require_auth(headers: &HeaderMap) -> Result<(), Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map(|_| ())
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

pub async fn get_dashboard_summary(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<DashboardSummary>, Response> {
    require_auth(&headers)?;

    let count_row = sqlx::query!("SELECT COUNT(*) as cnt FROM reports")
        .fetch_one(&state.pool)
        .await
        .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let metric_rows = sqlx::query!("SELECT DISTINCT metric FROM reports ORDER BY metric")
        .fetch_all(&state.pool)
        .await
        .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    Ok(Json(DashboardSummary {
        active_reports: count_row.cnt,
        core_metrics: metric_rows.into_iter().map(|r| r.metric).collect(),
    }))
}

pub async fn list_reports(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<SavedReport>>, Response> {
    require_auth(&headers)?;

    let rows = sqlx::query_as!(
        SavedReport,
        "SELECT id, name, description, metric, dimension, created_at, updated_at
         FROM reports ORDER BY created_at DESC"
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    Ok(Json(rows))
}

pub async fn get_report(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<SavedReport>, Response> {
    require_auth(&headers)?;

    let row = sqlx::query_as!(
        SavedReport,
        "SELECT id, name, description, metric, dimension, created_at, updated_at
         FROM reports WHERE id = ?",
        id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "report not found"))?;

    Ok(Json(row))
}

pub async fn create_report(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<CreateReportRequest>,
) -> Result<Response, Response> {
    require_auth(&headers)?;

    let name = req.name.trim().to_string();
    let metric = req.metric.trim().to_string();

    if name.is_empty() || metric.is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "name and metric are required",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query!(
        "INSERT INTO reports (id, name, description, metric, dimension, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        id,
        name,
        req.description,
        metric,
        req.dimension,
        now,
        now,
    )
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let created = sqlx::query_as!(
        SavedReport,
        "SELECT id, name, description, metric, dimension, created_at, updated_at
         FROM reports WHERE id = ?",
        id
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    Ok((StatusCode::CREATED, Json(created)).into_response())
}

pub async fn update_report(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateReportRequest>,
) -> Result<Json<SavedReport>, Response> {
    require_auth(&headers)?;

    let existing = sqlx::query_as!(
        SavedReport,
        "SELECT id, name, description, metric, dimension, created_at, updated_at
         FROM reports WHERE id = ?",
        id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "report not found"))?;

    let name = match req.name {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    "name cannot be empty",
                ));
            }
            t
        }
        None => existing.name.clone(),
    };

    let metric = match req.metric {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    "metric cannot be empty",
                ));
            }
            t
        }
        None => existing.metric.clone(),
    };

    let description = req
        .description
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.description);
    let dimension = req
        .dimension
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.dimension);
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query!(
        "UPDATE reports SET name = ?, description = ?, metric = ?, dimension = ?, updated_at = ?
         WHERE id = ?",
        name,
        description,
        metric,
        dimension,
        now,
        id
    )
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let updated = sqlx::query_as!(
        SavedReport,
        "SELECT id, name, description, metric, dimension, created_at, updated_at
         FROM reports WHERE id = ?",
        id
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    Ok(Json(updated))
}

pub async fn delete_report(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    require_auth(&headers)?;

    let result = sqlx::query!("DELETE FROM reports WHERE id = ?", id)
        .execute(&state.pool)
        .await
        .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    if result.rows_affected() == 0 {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "report not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
