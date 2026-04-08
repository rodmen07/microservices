use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    auth::validate_authorization_header,
    models::{
        Account, ApiError, CreateAccountRequest, ListAccountsQuery, ListAccountsResponse,
        UpdateAccountRequest, VALID_STATUSES,
    },
    AppState,
};

// Builds a JSON error response with the given HTTP status, error code, and message
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

// Validates the Bearer token in the request headers, returning an error response if invalid
fn require_auth(headers: &HeaderMap) -> Result<crate::auth::AuthClaims, Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());

    validate_authorization_header(header_value)
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

// Checks whether a status string is one of the accepted account status values
fn validate_status(status: &str) -> bool {
    VALID_STATUSES.contains(&status)
}

// Lists accounts with optional status and name-search filters, returning a paginated response
pub async fn list_accounts(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(params): Query<ListAccountsQuery>,
) -> Response {
    let claims = match require_auth(&headers) {
        Ok(claims) => claims,
        Err(resp) => return resp,
    };

    let limit = params.limit.unwrap_or(50).clamp(1, 100) as i64;
    let offset = params.offset.unwrap_or(0) as i64;

    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));
    let mut where_clauses = Vec::new();
    let mut params_vec: Vec<String> = Vec::new();
    let mut param_idx = 1usize;

    if let Some(status) = &params.status {
        where_clauses.push(format!("status = ${}", param_idx));
        param_idx += 1;
        params_vec.push(status.clone());
    }

    if let Some(q) = &params.q {
        where_clauses.push(format!("name LIKE ${}", param_idx));
        param_idx += 1;
        params_vec.push(format!("%{}%", q));
    }

    if !is_admin {
        where_clauses.push(format!("owner_id = ${}", param_idx));
        param_idx += 1;
        params_vec.push(claims.sub.clone());
    } else if let Some(owner_id) = &params.owner_id {
        where_clauses.push(format!("owner_id = ${}", param_idx));
        param_idx += 1;
        params_vec.push(owner_id.clone());
    }

    let mut query_base =
        "SELECT id, owner_id, name, domain, status, created_at, updated_at FROM accounts"
            .to_string();
    let mut count_base = "SELECT COUNT(*) FROM accounts".to_string();

    if !where_clauses.is_empty() {
        let where_stmt = format!(" WHERE {}", where_clauses.join(" AND "));
        query_base.push_str(&where_stmt);
        count_base.push_str(&where_stmt);
    }

    query_base.push_str(&format!(
        " ORDER BY created_at DESC, id DESC LIMIT ${} OFFSET ${}",
        param_idx,
        param_idx + 1
    ));

    let mut rows_query = sqlx::query_as::<_, Account>(&query_base);
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_base);

    for val in &params_vec {
        rows_query = rows_query.bind(val);
        count_query = count_query.bind(val);
    }

    rows_query = rows_query.bind(limit).bind(offset);

    let rows = rows_query.fetch_all(&state.pool).await;
    let total = count_query.fetch_one(&state.pool).await;

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

// Fetches a single account by ID, returning 404 if it does not exist
pub async fn get_account(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    let claims = match require_auth(&headers) {
        Ok(claims) => claims,
        Err(resp) => return resp,
    };

    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let query = if is_admin {
        "SELECT id, owner_id, name, domain, status, created_at, updated_at FROM accounts WHERE id = $1"
    } else {
        "SELECT id, owner_id, name, domain, status, created_at, updated_at FROM accounts WHERE id = $1 AND owner_id = $2"
    };

    let mut q = sqlx::query_as::<_, Account>(query).bind(&id);
    if !is_admin {
        q = q.bind(&claims.sub);
    }

    match q.fetch_optional(&state.pool).await {
        Ok(Some(account)) => Json(account).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "account not found"),
        Err(e) => {
            tracing::error!("get_account db error: {e}");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
        }
    }
}

// Validates and inserts a new account, returning the created record with HTTP 201
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
        return error_response(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "name is required",
        );
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

    let claims = match require_auth(&headers) {
        Ok(claims) => claims,
        Err(resp) => return resp,
    };

    let owner_id = claims.sub.clone();
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    match sqlx::query(
        "INSERT INTO accounts (id, owner_id, name, domain, status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&id)
    .bind(&owner_id)
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
        owner_id,
        name,
        domain,
        status,
        created_at: now.clone(),
        updated_at: now,
    };

    crate::pipeline::emit_event(
        state.http_client.clone(),
        "accounts",
        "account.created",
        serde_json::to_value(&account).unwrap_or_default(),
    );
    crate::pipeline::index_search_document(
        state.http_client.clone(),
        "account",
        account.id.clone(),
        account.name.clone(),
        format!(
            "domain: {} | status: {}",
            account.domain.as_deref().unwrap_or("-"),
            account.status
        ),
    );

    (StatusCode::CREATED, Json(account)).into_response()
}

// Applies partial updates to an existing account, merging provided fields with stored values
pub async fn update_account(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<UpdateAccountRequest>,
) -> Response {
    let claims = match require_auth(&headers) {
        Ok(claims) => claims,
        Err(resp) => return resp,
    };

    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    // Fetch existing account first.
    let existing = match sqlx::query_as::<_, Account>(
        "SELECT id, owner_id, name, domain, status, created_at, updated_at FROM accounts WHERE id = $1",
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

    if !is_admin && existing.owner_id != claims.sub {
        return error_response(StatusCode::FORBIDDEN, "FORBIDDEN", "not allowed");
    }

    let name = match body.name.as_deref().map(str::trim) {
        Some("") => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                "name cannot be empty",
            )
        }
        Some(n) => n.to_string(),
        None => existing.name.clone(),
    };

    let domain = match &body.domain {
        Some(d) => {
            let trimmed = d.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
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
        "UPDATE accounts SET name = $1, domain = $2, status = $3, updated_at = $4 WHERE id = $5",
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
        owner_id: existing.owner_id,
        name,
        domain,
        status,
        created_at: existing.created_at,
        updated_at: now,
    };

    crate::pipeline::emit_event(
        state.http_client.clone(),
        "accounts",
        "account.updated",
        serde_json::to_value(&updated).unwrap_or_default(),
    );
    crate::pipeline::index_search_document(
        state.http_client.clone(),
        "account",
        updated.id.clone(),
        updated.name.clone(),
        format!(
            "domain: {} | status: {}",
            updated.domain.as_deref().unwrap_or("-"),
            updated.status
        ),
    );

    Json(updated).into_response()
}

// Deletes an account by ID, returning 204 on success or 404 if not found
pub async fn delete_account(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    let claims = match require_auth(&headers) {
        Ok(claims) => claims,
        Err(resp) => return resp,
    };
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let result = if is_admin {
        sqlx::query("DELETE FROM accounts WHERE id = $1")
            .bind(&id)
            .execute(&state.pool)
            .await
    } else {
        sqlx::query("DELETE FROM accounts WHERE id = $1 AND owner_id = $2")
            .bind(&id)
            .bind(&claims.sub)
            .execute(&state.pool)
            .await
    };

    match result {
        Ok(result) if result.rows_affected() > 0 => {
            crate::pipeline::delete_search_document(state.http_client.clone(), id);
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(_) => error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "account not found"),
        Err(e) => {
            tracing::error!("delete_account db error: {e}");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
        }
    }
}
