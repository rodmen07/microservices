use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::validate_authorization_header,
    models::{ApiError, CreateOpportunityRequest, Opportunity, UpdateOpportunityRequest},
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
    let body = Json(ApiError {
        code: code.to_string(),
        message: message.to_string(),
        details: None,
    });
    (status, body).into_response()
}

// Validates the Bearer token in the request headers, returning an error response if invalid
fn require_auth(headers: &HeaderMap) -> Result<crate::auth::AuthClaims, Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

// Returns all opportunities ordered by creation date descending
pub async fn list_opportunities(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<Opportunity>>, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let (query, qs) = if is_admin {
        let base = "SELECT id, owner_id, account_id, name, stage, amount, close_date, created_at, updated_at FROM opportunities";
        if let Some(owner_id) = params.get("owner_id") {
            (
                format!("{} WHERE owner_id = $1 ORDER BY created_at DESC", base),
                Some(owner_id.clone()),
            )
        } else {
            (format!("{} ORDER BY created_at DESC", base), None)
        }
    } else {
        (
            "SELECT id, owner_id, account_id, name, stage, amount, close_date, created_at, updated_at FROM opportunities WHERE owner_id = $1 ORDER BY created_at DESC".to_string(),
            Some(claims.sub.clone()),
        )
    };

    let mut query_obj = sqlx::query_as::<_, Opportunity>(&query);
    if let Some(owner_id) = qs {
        query_obj = query_obj.bind(owner_id);
    }

    let rows = query_obj.fetch_all(&state.pool).await.map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    tracing::debug!(actor = %claims.sub, count = rows.len(), "list_opportunities ok");
    Ok(Json(rows))
}

// Fetches a single opportunity by ID, returning 404 if it does not exist
pub async fn get_opportunity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Opportunity>, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let q = if is_admin {
        sqlx::query_as::<_, Opportunity>("SELECT id, owner_id, account_id, name, stage, amount, close_date, created_at, updated_at FROM opportunities WHERE id = $1").bind(&id)
    } else {
        sqlx::query_as::<_, Opportunity>("SELECT id, owner_id, account_id, name, stage, amount, close_date, created_at, updated_at FROM opportunities WHERE id = $1 AND owner_id = $2").bind(&id).bind(&claims.sub)
    };

    let row = q
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
        })?
        .ok_or_else(|| {
            error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "opportunity not found")
        })?;

    tracing::debug!(opportunity_id = %row.id, actor = %claims.sub, "get_opportunity ok");
    Ok(Json(row))
}

// Validates and inserts a new opportunity, returning the created record with HTTP 201
pub async fn create_opportunity(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<CreateOpportunityRequest>,
) -> Result<Response, Response> {
    let claims = require_auth(&headers)?;
    let owner_id = claims.sub.clone();

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "name is required".to_string(),
                details: Some(serde_json::json!({ "field": "name", "constraint": "must not be empty" })),
            }),
        ).into_response());
    }
    if req.account_id.trim().is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "account_id is required".to_string(),
                details: Some(serde_json::json!({ "field": "account_id", "constraint": "must not be empty" })),
            }),
        ).into_response());
    }

    let stage = req
        .stage
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("qualification")
        .to_string();
    let amount = req.amount.unwrap_or(0.0);

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "INSERT INTO opportunities (id, owner_id, account_id, name, stage, amount, close_date, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(&id)
    .bind(&owner_id)
    .bind(&req.account_id)
    .bind(&name)
    .bind(&stage)
    .bind(amount)
    .bind(&req.close_date)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let created = sqlx::query_as::<_, Opportunity>(
        "SELECT id, owner_id, account_id, name, stage, amount,
                close_date, created_at, updated_at
         FROM opportunities WHERE id = $1",
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
        "opportunities",
        "opportunity.created",
        serde_json::to_value(&created).unwrap_or_default(),
    );
    crate::pipeline::index_search_document(
        state.http_client.clone(),
        "opportunity",
        created.id.clone(),
        created.name.clone(),
        format!(
            "stage: {} | amount: {} | account_id: {}",
            created.stage, created.amount, created.account_id
        ),
    );

    let auth_hdr = headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    emit_audit(&state.http_client, "opportunity", &created.id, "created", &owner_id, Some(&created.name), &auth_hdr).await;

    tracing::info!(opportunity_id = %created.id, actor = %claims.sub, opportunity_name = %created.name, "opportunity created");
    Ok((StatusCode::CREATED, Json(created)).into_response())
}

// Applies partial updates to an existing opportunity, merging provided fields with stored values
pub async fn update_opportunity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateOpportunityRequest>,
) -> Result<Json<Opportunity>, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let existing = {
        let mut q = sqlx::query_as::<_, Opportunity>(
            if is_admin {
                "SELECT id, owner_id, account_id, name, stage, amount, close_date, created_at, updated_at FROM opportunities WHERE id = $1"
            } else {
                "SELECT id, owner_id, account_id, name, stage, amount, close_date, created_at, updated_at FROM opportunities WHERE id = $1 AND owner_id = $2"
            },
        )
        .bind(&id);

        if !is_admin {
            q = q.bind(&claims.sub);
        }

        q.fetch_optional(&state.pool).await
    }
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "opportunity not found"))?;

    let name = match req.name {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(ApiError {
                        code: "VALIDATION_ERROR".to_string(),
                        message: "name cannot be empty".to_string(),
                        details: Some(serde_json::json!({ "field": "name", "constraint": "must not be empty" })),
                    }),
                ).into_response());
            }
            t
        }
        None => existing.name.clone(),
    };

    let stage = match req.stage {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(ApiError {
                        code: "VALIDATION_ERROR".to_string(),
                        message: "stage cannot be empty".to_string(),
                        details: Some(serde_json::json!({ "field": "stage", "constraint": "must not be empty" })),
                    }),
                ).into_response());
            }
            t
        }
        None => existing.stage.clone(),
    };

    let amount = req.amount.unwrap_or(existing.amount);
    let close_date = req
        .close_date
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.close_date);
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "UPDATE opportunities SET name = $1, stage = $2, amount = $3, close_date = $4, updated_at = $5 WHERE id = $6",
    )
    .bind(&name)
    .bind(&stage)
    .bind(amount)
    .bind(close_date)
    .bind(&now)
    .bind(&id)
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let updated = sqlx::query_as::<_, Opportunity>(
        "SELECT id, owner_id, account_id, name, stage, amount,
                close_date, created_at, updated_at
         FROM opportunities WHERE id = $1",
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
        "opportunities",
        "opportunity.updated",
        serde_json::to_value(&updated).unwrap_or_default(),
    );
    crate::pipeline::index_search_document(
        state.http_client.clone(),
        "opportunity",
        updated.id.clone(),
        updated.name.clone(),
        format!(
            "stage: {} | amount: {} | account_id: {}",
            updated.stage, updated.amount, updated.account_id
        ),
    );

    let auth_hdr = headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    emit_audit(&state.http_client, "opportunity", &updated.id, "updated", &updated.owner_id, Some(&updated.name), &auth_hdr).await;

    tracing::info!(opportunity_id = %updated.id, actor = %claims.sub, opportunity_name = %updated.name, "opportunity updated");
    Ok(Json(updated))
}

// Deletes an opportunity by ID, returning 204 on success or 404 if not found
pub async fn delete_opportunity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let result = if is_admin {
        sqlx::query("DELETE FROM opportunities WHERE id = $1")
            .bind(&id)
            .execute(&state.pool)
            .await
    } else {
        sqlx::query("DELETE FROM opportunities WHERE id = $1 AND owner_id = $2")
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
            "opportunity not found",
        ));
    }

    let auth_hdr = headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    emit_audit(&state.http_client, "opportunity", &id, "deleted", &claims.sub, None, &auth_hdr).await;
    crate::pipeline::delete_search_document(state.http_client.clone(), id.clone());

    tracing::info!(opportunity_id = %id, actor = %claims.sub, "opportunity deleted");
    Ok(StatusCode::NO_CONTENT)
}
