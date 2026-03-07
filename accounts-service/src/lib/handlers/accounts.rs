use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    AppState,
    auth::validate_authorization_header,
    models::{
        Account, ApiError, CreateAccountRequest, ListAccountsQuery, ListAccountsResponse,
        UpdateAccountRequest, VALID_STATUSES,
    },
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
    let header_value = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    validate_authorization_header(header_value).map(|_| ()).map_err(|err| {
        error_response(StatusCode::UNAUTHORIZED, err.code(), err.message())
    })
}

fn validate_status(status: &str) -> bool {
    VALID_STATUSES.contains(&status)
}

pub async fn list_accounts(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(params): Query<ListAccountsQuery>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let limit = params.limit.unwrap_or(50).clamp(1, 100) as i64;
    let offset = params.offset.unwrap_or(0) as i64;

    // Build dynamic query with optional filters.
    let (rows, total) = match (&params.status, &params.q) {
        (Some(status), Some(q)) => {
            let pattern = format!("%{}%", q);
            let rows = sqlx::query_as::<_, Account>(
                "SELECT id, name, domain, status, created_at, updated_at
                 FROM accounts
                 WHERE status = ? AND name LIKE ?
                 ORDER BY name ASC LIMIT ? OFFSET ?",
            )
            .bind(status)
            .bind(&pattern)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.pool)
            .await;
            let total = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM accounts WHERE status = ? AND name LIKE ?",
            )
            .bind(status)
            .bind(&pattern)
            .fetch_one(&state.pool)
            .await;
            (rows, total)
        }
        (Some(status), None) => {
            let rows = sqlx::query_as::<_, Account>(
                "SELECT id, name, domain, status, created_at, updated_at
                 FROM accounts WHERE status = ?
                 ORDER BY name ASC LIMIT ? OFFSET ?",
            )
            .bind(status)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.pool)
            .await;
            let total = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM accounts WHERE status = ?",
            )
            .bind(status)
            .fetch_one(&state.pool)
            .await;
            (rows, total)
        }
        (None, Some(q)) => {
            let pattern = format!("%{}%", q);
            let rows = sqlx::query_as::<_, Account>(
                "SELECT id, name, domain, status, created_at, updated_at
                 FROM accounts WHERE name LIKE ?
                 ORDER BY name ASC LIMIT ? OFFSET ?",
            )
            .bind(&pattern)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.pool)
            .await;
            let total = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM accounts WHERE name LIKE ?",
            )
            .bind(&pattern)
            .fetch_one(&state.pool)
            .await;
            (rows, total)
        }
        (None, None) => {
            let rows = sqlx::query_as::<_, Account>(
                "SELECT id, name, domain, status, created_at, updated_at
                 FROM accounts ORDER BY name ASC LIMIT ? OFFSET ?",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.pool)
            .await;
            let total =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM accounts")
                    .fetch_one(&state.pool)
                    .await;
            (rows, total)
        }
    };

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("list_accounts db error: {e}");
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            );
        }
    };

    let total = match total {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("list_accounts count error: {e}");
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            );
        }
    };

    Json(ListAccountsResponse {
        data: rows,
        total,
        limit: limit as u32,
        offset: offset as u32,
    })
    .into_response()
}

pub async fn get_account(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    match sqlx::query_as::<_, Account>(
        "SELECT id, name, domain, status, created_at, updated_at FROM accounts WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(account)) => Json(account).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "account not found"),
        Err(e) => {
            tracing::error!("get_account db error: {e}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error")
        }
    }
}

pub async fn create_account(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<CreateAccountRequest>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let name = body.name.trim().to_string();
    if name.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "VALIDATION_ERROR", "name is required");
    }

    let status = body
        .status
        .as_deref()
        .map(str::trim)
        .unwrap_or("active")
        .to_string();

    if !validate_status(&status) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "invalid status value".to_string(),
                details: Some(json!({ "valid_values": VALID_STATUSES })),
            }),
        )
            .into_response();
    }

    let domain = body
        .domain
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    match sqlx::query(
        "INSERT INTO accounts (id, name, domain, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&name)
    .bind(&domain)
    .bind(&status)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await
    {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("create_account db error: {e}");
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            );
        }
    }

    let account = Account {
        id,
        name,
        domain,
        status,
        created_at: now.clone(),
        updated_at: now,
    };

    (StatusCode::CREATED, Json(account)).into_response()
}

pub async fn update_account(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<UpdateAccountRequest>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    // Fetch existing account first.
    let existing = match sqlx::query_as::<_, Account>(
        "SELECT id, name, domain, status, created_at, updated_at FROM accounts WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(a)) => a,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "account not found"),
        Err(e) => {
            tracing::error!("update_account fetch error: {e}");
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            );
        }
    };

    let name = match body.name.as_deref().map(str::trim) {
        Some(n) if n.is_empty() => {
            return error_response(StatusCode::BAD_REQUEST, "VALIDATION_ERROR", "name cannot be empty")
        }
        Some(n) => n.to_string(),
        None => existing.name.clone(),
    };

    let domain = match &body.domain {
        Some(d) => {
            let trimmed = d.trim();
            if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
        }
        None => existing.domain.clone(),
    };

    let status = match body.status.as_deref().map(str::trim) {
        Some(s) => {
            if !validate_status(s) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ApiError {
                        code: "VALIDATION_ERROR".to_string(),
                        message: "invalid status value".to_string(),
                        details: Some(json!({ "valid_values": VALID_STATUSES })),
                    }),
                )
                    .into_response();
            }
            s.to_string()
        }
        None => existing.status.clone(),
    };

    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    match sqlx::query(
        "UPDATE accounts SET name = ?, domain = ?, status = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&name)
    .bind(&domain)
    .bind(&status)
    .bind(&now)
    .bind(&id)
    .execute(&state.pool)
    .await
    {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("update_account db error: {e}");
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            );
        }
    }

    let updated = Account {
        id: existing.id,
        name,
        domain,
        status,
        created_at: existing.created_at,
        updated_at: now,
    };

    Json(updated).into_response()
}

pub async fn delete_account(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    match sqlx::query("DELETE FROM accounts WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await
    {
        Ok(result) if result.rows_affected() > 0 => StatusCode::NO_CONTENT.into_response(),
        Ok(_) => error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "account not found"),
        Err(e) => {
            tracing::error!("delete_account db error: {e}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error")
        }
    }
}
