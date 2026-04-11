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

// Fire-and-forget audit event emission; errors are silently ignored
async fn emit_audit(
    client: &reqwest::Client,
    entity_type: &'static str,
    entity_id: &str,
    action: &'static str,
    actor_id: &str,
    entity_label: Option<&str>,
    auth_header: &str,
) {
    let Ok(url) = std::env::var("AUDIT_SERVICE_URL") else { return };
    if url.trim().is_empty() { return }
    let body = serde_json::json!({
        "entity_type": entity_type, "entity_id": entity_id,
        "action": action, "actor_id": actor_id, "entity_label": entity_label,
    });
    let _ = client
        .post(format!("{}/api/v1/audit-events", url.trim_end_matches('/')))
        .header("Authorization", auth_header)
        .json(&body)
        .send()
        .await;
}

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

    let claims = match require_auth(&headers) {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let name_pattern = params.q.as_deref().map(|q| format!("%{}%", q));

    let mut base = String::from(
        "SELECT id, owner_id, account_id, first_name, last_name, email, phone, lifecycle_stage, created_at, updated_at
         FROM contacts WHERE 1=1",
    );
    let mut count_base = String::from("SELECT COUNT(*) FROM contacts WHERE 1=1");
    let mut param_idx = 1usize;

    if params.account_id.is_some() {
        base.push_str(&format!(" AND account_id = ${}", param_idx));
        count_base.push_str(&format!(" AND account_id = ${}", param_idx));
        param_idx += 1;
    }
    if params.lifecycle_stage.is_some() {
        base.push_str(&format!(" AND lifecycle_stage = ${}", param_idx));
        count_base.push_str(&format!(" AND lifecycle_stage = ${}", param_idx));
        param_idx += 1;
    }
    if name_pattern.is_some() {
        base.push_str(&format!(
            " AND (first_name ILIKE ${p} OR last_name ILIKE ${p1} OR email ILIKE ${p2})",
            p = param_idx,
            p1 = param_idx + 1,
            p2 = param_idx + 2
        ));
        count_base.push_str(&format!(
            " AND (first_name ILIKE ${p} OR last_name ILIKE ${p1} OR email ILIKE ${p2})",
            p = param_idx,
            p1 = param_idx + 1,
            p2 = param_idx + 2
        ));
        param_idx += 3;
    }
    if !is_admin || params.owner_id.is_some() {
        base.push_str(&format!(" AND owner_id = ${}", param_idx));
        count_base.push_str(&format!(" AND owner_id = ${}", param_idx));
        param_idx += 1;
    }

    base.push_str(&format!(
        " ORDER BY last_name ASC, first_name ASC LIMIT ${} OFFSET ${}",
        param_idx,
        param_idx + 1
    ));

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
    if !is_admin {
        rows_query = rows_query.bind(&claims.sub);
        count_query = count_query.bind(&claims.sub);
    } else if let Some(owner_id) = &params.owner_id {
        rows_query = rows_query.bind(owner_id);
        count_query = count_query.bind(owner_id);
    }

    rows_query = rows_query.bind(limit).bind(offset);

    let rows = rows_query.fetch_all(&state.pool).await;
    let total = count_query.fetch_one(&state.pool).await;

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
    let claims = match require_auth(&headers) {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let query = if is_admin {
        "SELECT id, owner_id, account_id, first_name, last_name, email, phone, lifecycle_stage, created_at, updated_at FROM contacts WHERE id = $1"
    } else {
        "SELECT id, owner_id, account_id, first_name, last_name, email, phone, lifecycle_stage, created_at, updated_at FROM contacts WHERE id = $1 AND owner_id = $2"
    };

    let mut q = sqlx::query_as::<_, Contact>(query).bind(&id);
    if !is_admin {
        q = q.bind(&claims.sub);
    }

    match q.fetch_optional(&state.pool).await {
        Ok(Some(contact)) => Json(contact).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "contact not found"),
        Err(e) => {
            tracing::error!("get_contact db error: {e}");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
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
    let claims = match require_auth(&headers) {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let owner_id = claims.sub.clone();

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    match sqlx::query(
        "INSERT INTO contacts (id, owner_id, account_id, first_name, last_name, email, phone, lifecycle_stage, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(&id)
    .bind(&owner_id)
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
        owner_id,
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
    crate::pipeline::index_search_document(
        state.http_client.clone(),
        "contact",
        contact.id.clone(),
        format!("{} {}", contact.first_name, contact.last_name),
        format!(
            "email: {} | phone: {} | stage: {}",
            contact.email.as_deref().unwrap_or("-"),
            contact.phone.as_deref().unwrap_or("-"),
            contact.lifecycle_stage
        ),
    );

    let label = format!("{} {}", contact.first_name, contact.last_name);
    let auth_hdr = headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    emit_audit(&state.http_client, "contact", &contact.id, "created", &contact.owner_id, Some(&label), &auth_hdr).await;

    (StatusCode::CREATED, Json(contact)).into_response()
}

// Applies partial updates to an existing contact, with optional account re-validation
pub async fn update_contact(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<UpdateContactRequest>,
) -> Response {
    let claims = match require_auth(&headers) {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let existing = {
        let mut q = sqlx::query_as::<_, Contact>(
            if is_admin {
                "SELECT id, owner_id, account_id, first_name, last_name, email, phone, lifecycle_stage, created_at, updated_at FROM contacts WHERE id = $1"
            } else {
                "SELECT id, owner_id, account_id, first_name, last_name, email, phone, lifecycle_stage, created_at, updated_at FROM contacts WHERE id = $1 AND owner_id = $2"
            },
        )
        .bind(&id);

        if !is_admin {
            q = q.bind(&claims.sub);
        }

        q.fetch_optional(&state.pool).await
    };

    let existing = match existing {
        Ok(Some(c)) => c,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "contact not found"),
        Err(e) => {
            tracing::error!("update_contact fetch error: {e}");
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            );
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
        "UPDATE contacts SET account_id = $1, first_name = $2, last_name = $3, email = $4, phone = $5,
         lifecycle_stage = $6, updated_at = $7 WHERE id = $8",
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
        owner_id: existing.owner_id,
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
    crate::pipeline::index_search_document(
        state.http_client.clone(),
        "contact",
        updated.id.clone(),
        format!("{} {}", updated.first_name, updated.last_name),
        format!(
            "email: {} | phone: {} | stage: {}",
            updated.email.as_deref().unwrap_or("-"),
            updated.phone.as_deref().unwrap_or("-"),
            updated.lifecycle_stage
        ),
    );

    let label = format!("{} {}", updated.first_name, updated.last_name);
    let auth_hdr = headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    emit_audit(&state.http_client, "contact", &updated.id, "updated", &updated.owner_id, Some(&label), &auth_hdr).await;

    Json(updated).into_response()
}

// Deletes a contact by ID, returning 204 on success or 404 if not found
pub async fn delete_contact(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    let claims = match require_auth(&headers) {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let deletion = if is_admin {
        sqlx::query("DELETE FROM contacts WHERE id = $1")
            .bind(&id)
            .execute(&state.pool)
            .await
    } else {
        sqlx::query("DELETE FROM contacts WHERE id = $1 AND owner_id = $2")
            .bind(&id)
            .bind(&claims.sub)
            .execute(&state.pool)
            .await
    };

    match deletion {
        Ok(result) if result.rows_affected() > 0 => {
            let auth_hdr = headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
            emit_audit(&state.http_client, "contact", &id, "deleted", &claims.sub, None, &auth_hdr).await;
            crate::pipeline::delete_search_document(state.http_client.clone(), id);
            StatusCode::NO_CONTENT.into_response()
        }
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
