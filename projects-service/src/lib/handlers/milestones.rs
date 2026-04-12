use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use uuid::Uuid;
use serde_json::json;

use crate::{
    app_state::AppState,
    auth::{validate_authorization_header, AuthClaims},
    models::{ApiError, CreateMilestoneRequest, Milestone, Project, UpdateMilestoneRequest},
};

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = Json(ApiError {
        code: code.to_string(),
        message: message.to_string(),
        details: None,
    });
    (status, body).into_response()
}

fn require_auth_with_claims(headers: &HeaderMap) -> Result<AuthClaims, Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

fn require_admin(claims: &AuthClaims) -> Result<(), Response> {
    if claims.has_role("admin") {
        Ok(())
    } else {
        Err(error_response(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "admin role required",
        ))
    }
}

async fn require_project_access(
    pool: &sqlx::PgPool,
    project_id: &str,
    claims: &AuthClaims,
) -> Result<(), Response> {
    let project = sqlx::query_as::<_, Project>(
        "SELECT id, account_id, client_user_id, name, description, status,
                start_date, target_end_date, created_at, updated_at
         FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "project not found"))?;

    if claims.has_role("client") && project.client_user_id.as_deref() != Some(&claims.sub) {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "project not found",
        ));
    }
    Ok(())
}

const VALID_STATUSES: &[&str] = &["pending", "in_progress", "completed"];

pub async fn list_milestones(
    headers: HeaderMap,
    Path(project_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<Milestone>>, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_project_access(&state.pool, &project_id, &claims).await?;

    let rows = sqlx::query_as::<_, Milestone>(
        "SELECT id, project_id, name, description, due_date, status, sort_order,
                created_at, updated_at
         FROM milestones WHERE project_id = $1 ORDER BY sort_order ASC, created_at ASC",
    )
    .bind(&project_id)
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

pub async fn create_milestone(
    headers: HeaderMap,
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<CreateMilestoneRequest>,
) -> Result<Response, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_admin(&claims)?;

    // Verify project exists
    sqlx::query_as::<_, Project>(
        "SELECT id, account_id, client_user_id, name, description, status,
                start_date, target_end_date, created_at, updated_at
         FROM projects WHERE id = $1",
    )
    .bind(&project_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "project not found"))?;

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "name is required".to_string(),
                details: Some(json!({ "field": "name", "constraint": "must not be empty" })),
            }),
        )
            .into_response());
    }

    let status = req
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("pending")
        .to_string();

    if !VALID_STATUSES.contains(&status.as_str()) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "status must be one of: pending, in_progress, completed".to_string(),
                details: Some(json!({ "field": "status", "valid_values": VALID_STATUSES })),
            }),
        )
            .into_response());
    }

    // Cast to i32: the milestones.sort_order column is INTEGER (INT4); sqlx 0.8
    // encodes i64 as INT8 which PostgreSQL rejects at binding time.
    let sort_order = req.sort_order.unwrap_or(0) as i32;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "INSERT INTO milestones (id, project_id, name, description, due_date, status,
                                 sort_order, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&name)
    .bind(&req.description)
    .bind(&req.due_date)
    .bind(&status)
    .bind(sort_order)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    let created = sqlx::query_as::<_, Milestone>(
        "SELECT id, project_id, name, description, due_date, status, sort_order,
                created_at, updated_at
         FROM milestones WHERE id = $1",
    )
    .bind(&id)
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

pub async fn update_milestone(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateMilestoneRequest>,
) -> Result<Json<Milestone>, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_admin(&claims)?;

    let existing = sqlx::query_as::<_, Milestone>(
        "SELECT id, project_id, name, description, due_date, status, sort_order,
                created_at, updated_at
         FROM milestones WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "milestone not found"))?;

    let name = match req.name {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(ApiError {
                        code: "VALIDATION_ERROR".to_string(),
                        message: "name cannot be empty".to_string(),
                        details: Some(json!({ "field": "name", "constraint": "must not be empty" })),
                    }),
                )
                    .into_response());
            }
            t
        }
        None => existing.name.clone(),
    };

    let status = match req.status {
        Some(v) => {
            let t = v.trim().to_string();
            if !VALID_STATUSES.contains(&t.as_str()) {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(ApiError {
                        code: "VALIDATION_ERROR".to_string(),
                        message: "status must be one of: pending, in_progress, completed".to_string(),
                        details: Some(json!({ "field": "status", "valid_values": VALID_STATUSES })),
                    }),
                )
                    .into_response());
            }
            t
        }
        None => existing.status.clone(),
    };

    let description = req.description.or(existing.description);
    let due_date = req
        .due_date
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.due_date);
    let sort_order = req.sort_order.map(|v| v as i32).unwrap_or(existing.sort_order);
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "UPDATE milestones SET name = $1, description = $2, due_date = $3, status = $4,
                sort_order = $5, updated_at = $6
         WHERE id = $7",
    )
    .bind(&name)
    .bind(&description)
    .bind(&due_date)
    .bind(&status)
    .bind(sort_order)
    .bind(&now)
    .bind(&id)
    .execute(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    let updated = sqlx::query_as::<_, Milestone>(
        "SELECT id, project_id, name, description, due_date, status, sort_order,
                created_at, updated_at
         FROM milestones WHERE id = $1",
    )
    .bind(&id)
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

pub async fn delete_milestone(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_admin(&claims)?;

    let result = sqlx::query("DELETE FROM milestones WHERE id = $1")
        .bind(&id)
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
            "milestone not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
