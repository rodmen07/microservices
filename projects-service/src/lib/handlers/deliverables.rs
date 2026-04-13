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
    models::{
        ApiError, CreateDeliverableRequest, Deliverable, Milestone, Project,
        UpdateDeliverableRequest,
    },
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

const VALID_STATUSES: &[&str] = &["not_started", "in_progress", "in_review", "accepted"];

pub async fn list_deliverables(
    headers: HeaderMap,
    Path(milestone_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<Deliverable>>, Response> {
    let claims = require_auth_with_claims(&headers)?;

    // Verify milestone exists and client has access to the parent project
    let milestone = sqlx::query_as::<_, Milestone>(
        "SELECT id, project_id, name, description, due_date, status, sort_order,
                created_at, updated_at
         FROM milestones WHERE id = $1",
    )
    .bind(&milestone_id)
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

    if claims.has_role("client") {
        let project = sqlx::query_as::<_, Project>(
            "SELECT id, account_id, client_user_id, name, description, status,
                    start_date, target_end_date, created_at, updated_at
             FROM projects WHERE id = $1",
        )
        .bind(&milestone.project_id)
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

        if project.client_user_id.as_deref() != Some(&claims.sub) {
            return Err(error_response(
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                "milestone not found",
            ));
        }
    }

    let rows = sqlx::query_as::<_, Deliverable>(
        "SELECT id, milestone_id, name, description, status, estimated_hours, created_at, updated_at
         FROM deliverables WHERE milestone_id = $1 ORDER BY created_at ASC",
    )
    .bind(&milestone_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    tracing::debug!(milestone_id = %milestone_id, actor = %claims.sub, count = rows.len(), "list_deliverables ok");
    Ok(Json(rows))
}

pub async fn create_deliverable(
    headers: HeaderMap,
    Path(milestone_id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<CreateDeliverableRequest>,
) -> Result<Response, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_admin(&claims)?;

    // Verify milestone exists
    sqlx::query_as::<_, Milestone>(
        "SELECT id, project_id, name, description, due_date, status, sort_order,
                created_at, updated_at
         FROM milestones WHERE id = $1",
    )
    .bind(&milestone_id)
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
        .unwrap_or("not_started")
        .to_string();

    if !VALID_STATUSES.contains(&status.as_str()) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "status must be one of: not_started, in_progress, in_review, accepted".to_string(),
                details: Some(json!({ "field": "status", "valid_values": VALID_STATUSES })),
            }),
        )
            .into_response());
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "INSERT INTO deliverables (id, milestone_id, name, description, status,
                                    estimated_hours, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&id)
    .bind(&milestone_id)
    .bind(&name)
    .bind(&req.description)
    .bind(&status)
    .bind(req.estimated_hours)
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

    let created = sqlx::query_as::<_, Deliverable>(
        "SELECT id, milestone_id, name, description, status, estimated_hours, created_at, updated_at
         FROM deliverables WHERE id = $1",
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

    tracing::info!(deliverable_id = %id, milestone_id = %milestone_id, actor = %claims.sub, "deliverable created");
    Ok((StatusCode::CREATED, Json(created)).into_response())
}

pub async fn update_deliverable(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateDeliverableRequest>,
) -> Result<Json<Deliverable>, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_admin(&claims)?;

    let existing = sqlx::query_as::<_, Deliverable>(
        "SELECT id, milestone_id, name, description, status, estimated_hours, created_at, updated_at
         FROM deliverables WHERE id = $1",
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
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "deliverable not found"))?;

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
                        message: "status must be one of: not_started, in_progress, in_review, accepted".to_string(),
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
    let estimated_hours = req.estimated_hours.or(existing.estimated_hours);
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "UPDATE deliverables SET name = $1, description = $2, status = $3,
                estimated_hours = $4, updated_at = $5
         WHERE id = $6",
    )
    .bind(&name)
    .bind(&description)
    .bind(&status)
    .bind(estimated_hours)
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

    let updated = sqlx::query_as::<_, Deliverable>(
        "SELECT id, milestone_id, name, description, status, estimated_hours, created_at, updated_at
         FROM deliverables WHERE id = $1",
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

    tracing::info!(deliverable_id = %id, milestone_id = %existing.milestone_id, actor = %claims.sub, "deliverable updated");
    Ok(Json(updated))
}

pub async fn delete_deliverable(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_admin(&claims)?;

    let result = sqlx::query("DELETE FROM deliverables WHERE id = $1")
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
            "deliverable not found",
        ));
    }

    tracing::info!(deliverable_id = %id, actor = %claims.sub, "deliverable deleted");
    Ok(StatusCode::NO_CONTENT)
}
