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
    models::{ApiError, CreateWorkflowRequest, UpdateWorkflowRequest, Workflow},
};

// Builds a JSON error response with the given HTTP status, error code, and message
fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = Json(ApiError {
        code: code.to_string(),
        message: message.to_string(),
        details: None,
    });
    (status, body).into_response()
}

// Validates the Bearer token in the request headers, returning an error response if invalid
fn require_auth(headers: &HeaderMap) -> Result<(), Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map(|_| ())
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

// Returns all workflows ordered by creation date descending
pub async fn list_workflows(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<Workflow>>, Response> {
    require_auth(&headers)?;

    let rows = sqlx::query_as::<_, Workflow>(
        "SELECT id, name, trigger_event, action_type, enabled,
                created_at, updated_at
         FROM workflows ORDER BY created_at DESC",
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

// Fetches a single workflow by ID, returning 404 if it does not exist
pub async fn get_workflow(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Workflow>, Response> {
    require_auth(&headers)?;

    let row = sqlx::query_as::<_, Workflow>(
        "SELECT id, name, trigger_event, action_type, enabled,
                created_at, updated_at
         FROM workflows WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "workflow not found"))?;

    Ok(Json(row))
}

// Validates and inserts a new workflow with enabled=true, returning the created record with HTTP 201
pub async fn create_workflow(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<CreateWorkflowRequest>,
) -> Result<Response, Response> {
    require_auth(&headers)?;

    let name = req.name.trim().to_string();
    let trigger_event = req.trigger_event.trim().to_string();
    let action_type = req.action_type.trim().to_string();

    if name.is_empty() || trigger_event.is_empty() || action_type.is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "name, trigger_event, and action_type are required",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "INSERT INTO workflows (id, name, trigger_event, action_type, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, 1, ?, ?)",
    )
    .bind(&id)
    .bind(&name)
    .bind(&trigger_event)
    .bind(&action_type)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let created = sqlx::query_as::<_, Workflow>(
        "SELECT id, name, trigger_event, action_type, enabled,
                created_at, updated_at
         FROM workflows WHERE id = ?",
    )
    .bind(id)
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

// Applies partial updates to an existing workflow, merging provided fields with stored values
pub async fn update_workflow(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateWorkflowRequest>,
) -> Result<Json<Workflow>, Response> {
    require_auth(&headers)?;

    let existing = sqlx::query_as::<_, Workflow>(
        "SELECT id, name, trigger_event, action_type, enabled,
                created_at, updated_at
         FROM workflows WHERE id = ?",
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
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "workflow not found"))?;

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

    let trigger_event = match req.trigger_event {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    "trigger_event cannot be empty",
                ));
            }
            t
        }
        None => existing.trigger_event.clone(),
    };

    let action_type = match req.action_type {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    "action_type cannot be empty",
                ));
            }
            t
        }
        None => existing.action_type.clone(),
    };

    let enabled = req.enabled.unwrap_or(existing.enabled);
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "UPDATE workflows SET name = ?, trigger_event = ?, action_type = ?, enabled = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(&name)
    .bind(&trigger_event)
    .bind(&action_type)
    .bind(enabled)
    .bind(&now)
    .bind(&id)
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let updated = sqlx::query_as::<_, Workflow>(
        "SELECT id, name, trigger_event, action_type, enabled,
                created_at, updated_at
         FROM workflows WHERE id = ?",
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

// Deletes a workflow by ID, returning 204 on success or 404 if not found
pub async fn delete_workflow(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    require_auth(&headers)?;

    let result = sqlx::query("DELETE FROM workflows WHERE id = ?")
        .bind(id)
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
            "workflow not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
