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
    models::{Activity, ApiError, CreateActivityRequest, UpdateActivityRequest},
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

// Returns all activities ordered by creation date descending
pub async fn list_activities(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<Activity>>, Response> {
    require_auth(&headers)?;

    let rows = sqlx::query_as!(
        Activity,
        "SELECT id, account_id, contact_id, activity_type, subject, notes, due_at,
                completed as \"completed: bool\", created_at, updated_at
         FROM activities
         ORDER BY created_at DESC"
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

// Fetches a single activity by ID, returning 404 if it does not exist
pub async fn get_activity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Activity>, Response> {
    require_auth(&headers)?;

    let row = sqlx::query_as!(
        Activity,
        "SELECT id, account_id, contact_id, activity_type, subject, notes, due_at,
                completed as \"completed: bool\", created_at, updated_at
         FROM activities WHERE id = ?",
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
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "activity not found"))?;

    Ok(Json(row))
}

// Validates and inserts a new activity, returning the created record with HTTP 201
pub async fn create_activity(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<CreateActivityRequest>,
) -> Result<Response, Response> {
    require_auth(&headers)?;

    let activity_type = req.activity_type.trim().to_string();
    let subject = req.subject.trim().to_string();

    if activity_type.is_empty() || subject.is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "activity_type and subject are required",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query!(
        "INSERT INTO activities
            (id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, ?)",
        id,
        req.account_id,
        req.contact_id,
        activity_type,
        subject,
        req.notes,
        req.due_at,
        now,
        now,
    )
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let created = sqlx::query_as!(
        Activity,
        "SELECT id, account_id, contact_id, activity_type, subject, notes, due_at,
                completed as \"completed: bool\", created_at, updated_at
         FROM activities WHERE id = ?",
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

// Applies partial updates to an existing activity, merging provided fields with stored values
pub async fn update_activity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateActivityRequest>,
) -> Result<Json<Activity>, Response> {
    require_auth(&headers)?;

    // Verify exists
    let existing = sqlx::query_as!(
        Activity,
        "SELECT id, account_id, contact_id, activity_type, subject, notes, due_at,
                completed as \"completed: bool\", created_at, updated_at
         FROM activities WHERE id = ?",
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
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "activity not found"))?;

    let activity_type = match req.activity_type {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    "activity_type cannot be empty",
                ));
            }
            t
        }
        None => existing.activity_type.clone(),
    };

    let subject = match req.subject {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    "subject cannot be empty",
                ));
            }
            t
        }
        None => existing.subject.clone(),
    };

    let notes = req
        .notes
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.notes);
    let due_at = req
        .due_at
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.due_at);
    let completed = req.completed.unwrap_or(existing.completed);
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query!(
        "UPDATE activities SET activity_type = ?, subject = ?, notes = ?, due_at = ?,
         completed = ?, updated_at = ? WHERE id = ?",
        activity_type,
        subject,
        notes,
        due_at,
        completed,
        now,
        id
    )
    .execute(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    let updated = sqlx::query_as!(
        Activity,
        "SELECT id, account_id, contact_id, activity_type, subject, notes, due_at,
                completed as \"completed: bool\", created_at, updated_at
         FROM activities WHERE id = ?",
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

// Deletes an activity by ID, returning 204 on success or 404 if not found
pub async fn delete_activity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    require_auth(&headers)?;

    let result = sqlx::query!("DELETE FROM activities WHERE id = ?", id)
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
            "activity not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
