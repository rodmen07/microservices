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
    models::{ApiError, CreateConnectionRequest, IntegrationConnection, UpdateConnectionRequest},
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

// Returns all integration connections ordered by creation date descending
pub async fn list_connections(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<IntegrationConnection>>, Response> {
    require_auth(&headers)?;

    let rows = sqlx::query_as!(
        IntegrationConnection,
        "SELECT id, provider, account_ref, status, last_synced_at, created_at, updated_at
         FROM connections ORDER BY created_at DESC"
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

// Fetches a single integration connection by ID, returning 404 if it does not exist
pub async fn get_connection(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<IntegrationConnection>, Response> {
    require_auth(&headers)?;

    let row = sqlx::query_as!(
        IntegrationConnection,
        "SELECT id, provider, account_ref, status, last_synced_at, created_at, updated_at
         FROM connections WHERE id = ?",
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
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "connection not found"))?;

    Ok(Json(row))
}

// Validates and inserts a new integration connection with status "connected", returning the created record with HTTP 201
pub async fn create_connection(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<CreateConnectionRequest>,
) -> Result<Response, Response> {
    require_auth(&headers)?;

    let provider = req.provider.trim().to_string();
    let account_ref = req.account_ref.trim().to_string();

    if provider.is_empty() || account_ref.is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "provider and account_ref are required",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query!(
        "INSERT INTO connections (id, provider, account_ref, status, last_synced_at, created_at, updated_at)
         VALUES (?, ?, ?, 'connected', NULL, ?, ?)",
        id,
        provider,
        account_ref,
        now,
        now,
    )
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let created = sqlx::query_as!(
        IntegrationConnection,
        "SELECT id, provider, account_ref, status, last_synced_at, created_at, updated_at
         FROM connections WHERE id = ?",
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

// Updates the status and last_synced_at of an existing connection, merging with stored values
pub async fn update_connection(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateConnectionRequest>,
) -> Result<Json<IntegrationConnection>, Response> {
    require_auth(&headers)?;

    let existing = sqlx::query_as!(
        IntegrationConnection,
        "SELECT id, provider, account_ref, status, last_synced_at, created_at, updated_at
         FROM connections WHERE id = ?",
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
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "connection not found"))?;

    let status = match req.status {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    "status cannot be empty",
                ));
            }
            t
        }
        None => existing.status.clone(),
    };

    let last_synced_at = req
        .last_synced_at
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.last_synced_at);
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query!(
        "UPDATE connections SET status = ?, last_synced_at = ?, updated_at = ? WHERE id = ?",
        status,
        last_synced_at,
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
        IntegrationConnection,
        "SELECT id, provider, account_ref, status, last_synced_at, created_at, updated_at
         FROM connections WHERE id = ?",
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

// Deletes an integration connection by ID, returning 204 on success or 404 if not found
pub async fn delete_connection(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    require_auth(&headers)?;

    let result = sqlx::query!("DELETE FROM connections WHERE id = ?", id)
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
            "connection not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
