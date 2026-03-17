use std::env;

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
        ApiError, Contact, CreateContactRequest, ListContactsQuery, ListContactsResponse,
        UpdateContactRequest, VALID_LIFECYCLE_STAGES,
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
fn require_auth(headers: &HeaderMap) -> Result<(), Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());

    validate_authorization_header(header_value)
        .map(|_| ())
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

// Checks whether a lifecycle stage string is one of the accepted values
fn validate_lifecycle_stage(stage: &str) -> bool {
    VALID_LIFECYCLE_STAGES.contains(&stage)
}

/// Verify that an account_id exists in the accounts service.
/// Returns Ok(true) if it exists, Ok(false) if not found, Err if the call fails.
/// Silently passes if ACCOUNTS_SERVICE_URL is not configured (fail-open for local dev).
async fn account_exists(client: &reqwest::Client, account_id: &str, auth_header: &str) -> bool {
    let base_url = match env::var("ACCOUNTS_SERVICE_URL") {
        Ok(url) => url,
        Err(_) => return true, // fail-open: no accounts service configured
    };

    let url = format!(
        "{}/api/v1/accounts/{}",
        base_url.trim_end_matches('/'),
        account_id
    );

    match client
        .get(&url)
        .header("Authorization", auth_header)
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(e) => {
            tracing::warn!("account validation request failed: {e}");
            false
        }
    }
}

// Lists contacts with optional account, lifecycle stage, and name/email search filters, returning a paginated response
pub async fn list_contacts(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(params): Query<ListContactsQuery>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let limit = params.limit.unwrap_or(50).clamp(1, 100) as i64;
    let offset = params.offset.unwrap_or(0) as i64;

    // Build query with up to three optional filters.
    let name_pattern = params.q.as_deref().map(|q| format!("%{}%", q));

    let (rows, total) = {
        let mut base = String::from(
            "SELECT id, account_id, first_name, last_name, email, phone, lifecycle_stage, created_at, updated_at
             FROM contacts WHERE 1=1",
        );
        let mut count_base = String::from("SELECT COUNT(*) FROM contacts WHERE 1=1");

        if params.account_id.is_some() {
            base.push_str(" AND account_id = ?");
            count_base.push_str(" AND account_id = ?");
        }
        if params.lifecycle_stage.is_some() {
            base.push_str(" AND lifecycle_stage = ?");
            count_base.push_str(" AND lifecycle_stage = ?");
        }
        if name_pattern.is_some() {
            base.push_str(
                " AND (first_name LIKE ? OR last_name LIKE ? OR email LIKE ?)",
            );
            count_base.push_str(
                " AND (first_name LIKE ? OR last_name LIKE ? OR email LIKE ?)",
            );
        }
        base.push_str(" ORDER BY last_name ASC, first_name ASC LIMIT ? OFFSET ?");

        // Bind parameters in the same order as the WHERE clauses.
        let mut rows_query = sqlx::query_as::<_, Contact>(&base);
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_base);

        if let Some(ref account_id) = params.account_id {
            rows_query = rows_query.bind(account_id);
            count_query = count_query.bind(account_id);
        }
        if let Some(ref stage) = params.lifecycle_stage {
            rows_query = rows_query.bind(stage);
            count_query = count_query.bind(stage);
        }
        if let Some(ref pattern) = name_pattern {
            rows_query = rows_query.bind(pattern).bind(pattern).bind(pattern);
            count_query = count_query.bind(pattern).bind(pattern).bind(pattern);
        }

        rows_query = rows_query.bind(limit).bind(offset);

        let rows = rows_query.fetch_all(&state.pool).await;
        let total = count_query.fetch_one(&state.pool).await;
        (rows, total)
    };

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("list_contacts db error: {e}");
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
            tracing::error!("list_contacts count error: {e}");
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            );
        }
    };

    Json(ListContactsResponse {
        data: rows,
        total,
        limit: limit as u32,
        offset: offset as u32,
    })
    .into_response()
}

// Fetches a single contact by ID, returning 404 if it does not exist
pub async fn get_contact(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    match sqlx::query_as::<_, Contact>(
        "SELECT id, account_id, first_name, last_name, email, phone, lifecycle_stage, created_at, updated_at
         FROM contacts WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(contact)) => Json(contact).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "contact not found"),
        Err(e) => {
            tracing::error!("get_contact db error: {e}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error")
        }
    }
}

// Validates and inserts a new contact, optionally verifying the account_id against the accounts service
pub async fn create_contact(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<CreateContactRequest>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let first_name = body.first_name.trim().to_string();
    let last_name = body.last_name.trim().to_string();

    if first_name.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "first_name is required",
        );
    }
    if last_name.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "last_name is required",
        );
    }

    let lifecycle_stage = body
        .lifecycle_stage
        .as_deref()
        .map(str::trim)
        .unwrap_or("lead")
        .to_string();

    if !validate_lifecycle_stage(&lifecycle_stage) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "invalid lifecycle_stage value".to_string(),
                details: Some(json!({ "valid_values": VALID_LIFECYCLE_STAGES })),
            }),
        )
            .into_response();
    }

    // Validate account_id exists in the accounts service (if provided).
    let account_id = body
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(aid) = account_id {
        let auth_header = headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !account_exists(&state.http_client, aid, auth_header).await {
            return error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_ACCOUNT",
                "account not found",
            );
        }
    }

    let email = body
        .email
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let phone = body
        .phone
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    match sqlx::query(
        "INSERT INTO contacts (id, account_id, first_name, last_name, email, phone, lifecycle_stage, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(account_id)
    .bind(&first_name)
    .bind(&last_name)
    .bind(&email)
    .bind(&phone)
    .bind(&lifecycle_stage)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await
    {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("create_contact db error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    }

    let contact = Contact {
        id,
        account_id: account_id.map(str::to_string),
        first_name,
        last_name,
        email,
        phone,
        lifecycle_stage,
        created_at: now.clone(),
        updated_at: now,
    };

    crate::pipeline::emit_event(
        state.http_client.clone(),
        "contacts",
        "contact.created",
        serde_json::to_value(&contact).unwrap_or_default(),
    );

    (StatusCode::CREATED, Json(contact)).into_response()
}

// Applies partial updates to an existing contact, with optional account re-validation
pub async fn update_contact(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<UpdateContactRequest>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let existing = match sqlx::query_as::<_, Contact>(
        "SELECT id, account_id, first_name, last_name, email, phone, lifecycle_stage, created_at, updated_at
         FROM contacts WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(c)) => c,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "contact not found"),
        Err(e) => {
            tracing::error!("update_contact fetch error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    };

    let first_name = match body.first_name.as_deref().map(str::trim) {
        Some("") => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                "first_name cannot be empty",
            )
        }
        Some(n) => n.to_string(),
        None => existing.first_name.clone(),
    };

    let last_name = match body.last_name.as_deref().map(str::trim) {
        Some("") => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                "last_name cannot be empty",
            )
        }
        Some(n) => n.to_string(),
        None => existing.last_name.clone(),
    };

    let lifecycle_stage = match body.lifecycle_stage.as_deref().map(str::trim) {
        Some(s) => {
            if !validate_lifecycle_stage(s) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ApiError {
                        code: "VALIDATION_ERROR".to_string(),
                        message: "invalid lifecycle_stage value".to_string(),
                        details: Some(json!({ "valid_values": VALID_LIFECYCLE_STAGES })),
                    }),
                )
                    .into_response();
            }
            s.to_string()
        }
        None => existing.lifecycle_stage.clone(),
    };

    // account_id: None means "don't change"; Some("") means "clear"; Some("id") means "set to id"
    let account_id: Option<Option<String>> = match &body.account_id {
        None => None, // no-op — keep existing
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Some(None) // clear account link
            } else {
                // Validate account exists.
                let auth_header = headers
                    .get("Authorization")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                if !account_exists(&state.http_client, trimmed, auth_header).await {
                    return error_response(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "INVALID_ACCOUNT",
                        "account not found",
                    );
                }
                Some(Some(trimmed.to_string()))
            }
        }
    };

    let new_account_id = match account_id {
        None => existing.account_id.clone(),
        Some(v) => v,
    };

    let email = match &body.email {
        Some(e) => {
            let t = e.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        None => existing.email.clone(),
    };

    let phone = match &body.phone {
        Some(p) => {
            let t = p.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        None => existing.phone.clone(),
    };

    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    match sqlx::query(
        "UPDATE contacts SET account_id = ?, first_name = ?, last_name = ?, email = ?, phone = ?,
         lifecycle_stage = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&new_account_id)
    .bind(&first_name)
    .bind(&last_name)
    .bind(&email)
    .bind(&phone)
    .bind(&lifecycle_stage)
    .bind(&now)
    .bind(&id)
    .execute(&state.pool)
    .await
    {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("update_contact db error: {e}");
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            );
        }
    }

    let updated = Contact {
        id: existing.id,
        account_id: new_account_id,
        first_name,
        last_name,
        email,
        phone,
        lifecycle_stage,
        created_at: existing.created_at,
        updated_at: now,
    };

    crate::pipeline::emit_event(
        state.http_client.clone(),
        "contacts",
        "contact.updated",
        serde_json::to_value(&updated).unwrap_or_default(),
    );

    Json(updated).into_response()
}

// Deletes a contact by ID, returning 204 on success or 404 if not found
pub async fn delete_contact(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    match sqlx::query("DELETE FROM contacts WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await
    {
        Ok(result) if result.rows_affected() > 0 => StatusCode::NO_CONTENT.into_response(),
        Ok(_) => error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "contact not found"),
        Err(e) => {
            tracing::error!("delete_contact db error: {e}");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
        }
    }
}
