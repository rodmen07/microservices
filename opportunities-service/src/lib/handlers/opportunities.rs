use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::validate_authorization_header,
    models::{ApiError, CreateOpportunityRequest, Opportunity, UpdateOpportunityRequest},
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

pub async fn list_opportunities(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<Opportunity>>, Response> {
    require_auth(&headers)?;

    let rows = sqlx::query_as!(
        Opportunity,
        "SELECT id, account_id, name, stage, amount as \"amount: f64\",
                close_date, created_at, updated_at
         FROM opportunities ORDER BY created_at DESC"
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    Ok(Json(rows))
}

pub async fn get_opportunity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Opportunity>, Response> {
    require_auth(&headers)?;

    let row = sqlx::query_as!(
        Opportunity,
        "SELECT id, account_id, name, stage, amount as \"amount: f64\",
                close_date, created_at, updated_at
         FROM opportunities WHERE id = ?",
        id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "opportunity not found"))?;

    Ok(Json(row))
}

pub async fn create_opportunity(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<CreateOpportunityRequest>,
) -> Result<Response, Response> {
    require_auth(&headers)?;

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "name is required",
        ));
    }
    if req.account_id.trim().is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "account_id is required",
        ));
    }

    let stage = req
        .stage
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("qualification")
        .to_string();
    let amount = req.amount.unwrap_or(0.0);

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query!(
        "INSERT INTO opportunities (id, account_id, name, stage, amount, close_date, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        id,
        req.account_id,
        name,
        stage,
        amount,
        req.close_date,
        now,
        now,
    )
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let created = sqlx::query_as!(
        Opportunity,
        "SELECT id, account_id, name, stage, amount as \"amount: f64\",
                close_date, created_at, updated_at
         FROM opportunities WHERE id = ?",
        id
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    Ok((StatusCode::CREATED, Json(created)).into_response())
}

pub async fn update_opportunity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateOpportunityRequest>,
) -> Result<Json<Opportunity>, Response> {
    require_auth(&headers)?;

    let existing = sqlx::query_as!(
        Opportunity,
        "SELECT id, account_id, name, stage, amount as \"amount: f64\",
                close_date, created_at, updated_at
         FROM opportunities WHERE id = ?",
        id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "opportunity not found"))?;

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

    let stage = match req.stage {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    "stage cannot be empty",
                ));
            }
            t
        }
        None => existing.stage.clone(),
    };

    let amount = req.amount.unwrap_or(existing.amount);
    let close_date = req
        .close_date
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.close_date);
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query!(
        "UPDATE opportunities SET name = ?, stage = ?, amount = ?, close_date = ?, updated_at = ? WHERE id = ?",
        name,
        stage,
        amount,
        close_date,
        now,
        id
    )
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let updated = sqlx::query_as!(
        Opportunity,
        "SELECT id, account_id, name, stage, amount as \"amount: f64\",
                close_date, created_at, updated_at
         FROM opportunities WHERE id = ?",
        id
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    Ok(Json(updated))
}

pub async fn delete_opportunity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    require_auth(&headers)?;

    let result = sqlx::query!("DELETE FROM opportunities WHERE id = ?", id)
        .execute(&state.pool)
        .await
        .map_err(|_| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "opportunity not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
