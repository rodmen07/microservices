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

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(ApiError {
            code: code.to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn require_auth(headers: &HeaderMap) -> Result<(), Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map(|_| ())
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

pub async fn list_activities(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<Activity>>, Response> {
    require_auth(&headers)?;

    let rows = sqlx::query_as::<_, Activity>(
        "SELECT id, account_id, contact_id, activity_type, subject, notes, due_at,
                completed, created_at, updated_at
         FROM activities ORDER BY created_at DESC",
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

pub async fn get_activity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Activity>, Response> {
    require_auth(&headers)?;

    sqlx::query_as::<_, Activity>(
        "SELECT id, account_id, contact_id, activity_type, subject, notes, due_at,
                completed, created_at, updated_at
         FROM activities WHERE id = $1",
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
    .map(Json)
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "activity not found"))
}

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

    sqlx::query(
        "INSERT INTO activities
            (id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, false, $8, $9)",
    )
    .bind(&id)
    .bind(&req.account_id)
    .bind(&req.contact_id)
    .bind(&activity_type)
    .bind(&subject)
    .bind(&req.notes)
    .bind(&req.due_at)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let created = sqlx::query_as::<_, Activity>(
        "SELECT id, account_id, contact_id, activity_type, subject, notes, due_at,
                completed, created_at, updated_at
         FROM activities WHERE id = $1",
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

pub async fn update_activity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateActivityRequest>,
) -> Result<Json<Activity>, Response> {
    require_auth(&headers)?;

    let existing = sqlx::query_as::<_, Activity>(
        "SELECT id, account_id, contact_id, activity_type, subject, notes, due_at,
                completed, created_at, updated_at
         FROM activities WHERE id = $1",
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

    sqlx::query(
        "UPDATE activities SET activity_type = $1, subject = $2, notes = $3, due_at = $4,
         completed = $5, updated_at = $6 WHERE id = $7",
    )
    .bind(&activity_type)
    .bind(&subject)
    .bind(&notes)
    .bind(&due_at)
    .bind(completed)
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

    let updated = sqlx::query_as::<_, Activity>(
        "SELECT id, account_id, contact_id, activity_type, subject, notes, due_at,
                completed, created_at, updated_at
         FROM activities WHERE id = $1",
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

pub async fn delete_activity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    require_auth(&headers)?;

    let result = sqlx::query("DELETE FROM activities WHERE id = $1")
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
            "activity not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
