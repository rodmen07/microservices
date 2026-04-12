use std::env;

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

async fn account_exists(client: &reqwest::Client, account_id: &str, auth_header: &str) -> bool {
    let base_url = match env::var("ACCOUNTS_SERVICE_URL") {
        Ok(url) => url,
        Err(_) => return true, // fail-open: no accounts service configured
    };
    let url = format!("{}/api/v1/accounts/{}", base_url.trim_end_matches('/'), account_id);
    match client.get(&url).header("Authorization", auth_header).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(e) => {
            tracing::warn!("account validation request failed: {e}");
            false
        }
    }
}

async fn contact_exists(client: &reqwest::Client, contact_id: &str, auth_header: &str) -> bool {
    let base_url = match env::var("CONTACTS_SERVICE_URL") {
        Ok(url) => url,
        Err(_) => return true, // fail-open: no contacts service configured
    };
    let url = format!("{}/api/v1/contacts/{}", base_url.trim_end_matches('/'), contact_id);
    match client.get(&url).header("Authorization", auth_header).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(e) => {
            tracing::warn!("contact validation request failed: {e}");
            false
        }
    }
}

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

fn require_auth(headers: &HeaderMap) -> Result<crate::auth::AuthClaims, Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

pub async fn list_activities(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<Activity>>, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let q = if is_admin {
        sqlx::query_as::<_, Activity>("SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at FROM activities ORDER BY created_at DESC")
    } else {
        sqlx::query_as::<_, Activity>("SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at FROM activities WHERE owner_id = $1 ORDER BY created_at DESC").bind(&claims.sub)
    };

    let rows = q.fetch_all(&state.pool).await.map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;
    tracing::debug!(actor = %claims.sub, count = rows.len(), "list_activities ok");

    Ok(Json(rows))
}

pub async fn get_activity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Activity>, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let q = if is_admin {
        sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at FROM activities WHERE id = $1",
        )
        .bind(&id)
    } else {
        sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at FROM activities WHERE id = $1 AND owner_id = $2",
        )
        .bind(&id)
        .bind(&claims.sub)
    };

    let activity = q.fetch_optional(&state.pool)
        .await
        .map_err(|_| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
        })?
        .map(Json)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "activity not found"))?;

    tracing::debug!(activity_id = %id, actor = %claims.sub, "get_activity ok");
    Ok(activity)
}

pub async fn create_activity(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<CreateActivityRequest>,
) -> Result<Response, Response> {
    let claims = require_auth(&headers)?;
    let owner_id = claims.sub.clone();

    let activity_type = req.activity_type.trim().to_string();
    let subject = req.subject.trim().to_string();

    if activity_type.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "activity_type is required".to_string(),
                details: Some(serde_json::json!({ "field": "activity_type", "constraint": "must not be empty" })),
            }),
        )
            .into_response());
    }
    if subject.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "subject is required".to_string(),
                details: Some(serde_json::json!({ "field": "subject", "constraint": "must not be empty" })),
            }),
        )
            .into_response());
    }

    let auth_header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if let Some(ref account_id) = req.account_id {
        if !account_exists(&state.http_client, account_id, auth_header).await {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ApiError {
                    code: "INVALID_ACCOUNT".to_string(),
                    message: "referenced account does not exist".to_string(),
                    details: Some(serde_json::json!({ "field": "account_id", "value": account_id })),
                }),
            )
                .into_response());
        }
    }

    if let Some(ref contact_id) = req.contact_id {
        if !contact_exists(&state.http_client, contact_id, auth_header).await {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ApiError {
                    code: "INVALID_CONTACT".to_string(),
                    message: "referenced contact does not exist".to_string(),
                    details: Some(serde_json::json!({ "field": "contact_id", "value": contact_id })),
                }),
            )
                .into_response());
        }
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "INSERT INTO activities
            (id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, $9, $10)",
    )
    .bind(&id)
    .bind(&owner_id)
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
        "SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at,
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

    crate::pipeline::emit_event(
        state.http_client.clone(),
        "activities",
        "activity.created",
        serde_json::to_value(&created).unwrap_or_default(),
    );
    crate::pipeline::index_search_document(
        state.http_client.clone(),
        "activity",
        created.id.clone(),
        created.subject.clone(),
        format!(
            "type: {} | notes: {}",
            created.activity_type,
            created.notes.as_deref().unwrap_or("-")
        ),
    );

    emit_audit(&state.http_client, "activity", &created.id, "created", &owner_id, Some(&created.subject), auth_header).await;
    tracing::info!(activity_id = %created.id, actor = %owner_id, "activity created");

    Ok((StatusCode::CREATED, Json(created)).into_response())
}

pub async fn update_activity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateActivityRequest>,
) -> Result<Json<Activity>, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let existing = {
        let mut q = sqlx::query_as::<_, Activity>(
            if is_admin {
                "SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at FROM activities WHERE id = $1"
            } else {
                "SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at FROM activities WHERE id = $1 AND owner_id = $2"
            },
        )
        .bind(&id);

        if !is_admin {
            q = q.bind(&claims.sub);
        }

        q.fetch_optional(&state.pool)
            .await
            .map_err(|_| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DB_ERROR",
                    "database error",
                )
            })?
            .ok_or_else(|| {
                error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "activity not found")
            })?
    };

    let activity_type = match req.activity_type {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(ApiError {
                        code: "VALIDATION_ERROR".to_string(),
                        message: "activity_type cannot be empty".to_string(),
                        details: Some(serde_json::json!({ "field": "activity_type", "constraint": "must not be empty" })),
                    }),
                )
                    .into_response());
            }
            t
        }
        None => existing.activity_type.clone(),
    };

    let subject = match req.subject {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(ApiError {
                        code: "VALIDATION_ERROR".to_string(),
                        message: "subject cannot be empty".to_string(),
                        details: Some(serde_json::json!({ "field": "subject", "constraint": "must not be empty" })),
                    }),
                )
                    .into_response());
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
        "SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at,
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

    crate::pipeline::emit_event(
        state.http_client.clone(),
        "activities",
        "activity.updated",
        serde_json::to_value(&updated).unwrap_or_default(),
    );
    crate::pipeline::index_search_document(
        state.http_client.clone(),
        "activity",
        updated.id.clone(),
        updated.subject.clone(),
        format!(
            "type: {} | notes: {}",
            updated.activity_type,
            updated.notes.as_deref().unwrap_or("-")
        ),
    );

    let auth_hdr = headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    emit_audit(&state.http_client, "activity", &updated.id, "updated", &updated.owner_id, Some(&updated.subject), &auth_hdr).await;
    tracing::info!(activity_id = %updated.id, actor = %claims.sub, "activity updated");

    Ok(Json(updated))
}

pub async fn delete_activity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let result = if is_admin {
        sqlx::query("DELETE FROM activities WHERE id = $1")
            .bind(&id)
            .execute(&state.pool)
            .await
    } else {
        sqlx::query("DELETE FROM activities WHERE id = $1 AND owner_id = $2")
            .bind(&id)
            .bind(&claims.sub)
            .execute(&state.pool)
            .await
    }
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

    let auth_hdr = headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    emit_audit(&state.http_client, "activity", &id, "deleted", &claims.sub, None, &auth_hdr).await;
    crate::pipeline::delete_search_document(state.http_client.clone(), id.clone());
    tracing::info!(activity_id = %id, actor = %claims.sub, "activity deleted");
    Ok(StatusCode::NO_CONTENT)
}
