use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    auth::validate_authorization_header,
    models::{
        ApiError, AuditEvent, CreateAuditEventRequest, ListAuditEventsQuery,
        ListAuditEventsResponse, VALID_ACTIONS, VALID_ENTITY_TYPES,
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

// Validates the Bearer token in the request headers, returning claims or an error response
fn require_auth(headers: &HeaderMap) -> Result<crate::auth::AuthClaims, Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

// Ingests a new audit event; any valid JWT may call this (CRM services forward the caller's token)
pub async fn ingest_audit_event(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<CreateAuditEventRequest>,
) -> Response {
    let claims = match require_auth(&headers) {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let entity_type = body.entity_type.trim().to_lowercase();
    if !VALID_ENTITY_TYPES.contains(&entity_type.as_str()) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "invalid entity_type".to_string(),
                details: Some(serde_json::json!({ "field": "entity_type", "valid_values": VALID_ENTITY_TYPES })),
            }),
        )
            .into_response();
    }

    let action = body.action.trim().to_lowercase();
    if !VALID_ACTIONS.contains(&action.as_str()) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "invalid action".to_string(),
                details: Some(serde_json::json!({ "field": "action", "valid_values": VALID_ACTIONS })),
            }),
        )
            .into_response();
    }

    let entity_id = body.entity_id.trim().to_string();
    if entity_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "entity_id is required".to_string(),
                details: Some(serde_json::json!({ "field": "entity_id", "constraint": "must not be empty" })),
            }),
        )
            .into_response();
    }

    let actor_id = body.actor_id.trim().to_string();
    if actor_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "actor_id is required".to_string(),
                details: Some(serde_json::json!({ "field": "actor_id", "constraint": "must not be empty" })),
            }),
        )
            .into_response();
    }

    let entity_label = body.entity_label.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let payload = body.payload.as_ref().map(|v| v.to_string());

    let id = Uuid::new_v4().to_string();
    let created_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    match sqlx::query(
        "INSERT INTO audit_events (id, entity_type, entity_id, action, actor_id, entity_label, payload, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&id)
    .bind(&entity_type)
    .bind(&entity_id)
    .bind(&action)
    .bind(&actor_id)
    .bind(&entity_label)
    .bind(&payload)
    .bind(&created_at)
    .execute(&state.pool)
    .await
    {
        Ok(_) => {
            tracing::info!(audit_event_id = %id, entity_id = %entity_id, actor_id = %actor_id, actor = %claims.sub, "audit event ingested");
        }
        Err(e) => {
            tracing::error!("ingest_audit_event db error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    }

    let event = AuditEvent {
        id,
        entity_type,
        entity_id,
        action,
        actor_id,
        entity_label,
        payload,
        created_at,
    };

    // Fire-and-forget: forward to Observaboard if configured
    if let (Some(url), Some(key)) = (
        state.observaboard_ingest_url.clone(),
        state.observaboard_api_key.clone(),
    ) {
        let client = state.http.clone();
        let event_type = format!("{}.{}", event.entity_type, event.action);
        let obs_payload = serde_json::json!({
            "source": "infraportal-crm",
            "event_type": event_type,
            "payload": {
                "audit_event_id": event.id,
                "entity_type": event.entity_type,
                "entity_id": event.entity_id,
                "action": event.action,
                "actor_id": event.actor_id,
                "entity_label": event.entity_label,
            }
        });
        tokio::spawn(async move {
            let result = client
                .post(&url)
                .header("Authorization", format!("Api-Key {key}"))
                .json(&obs_payload)
                .send()
                .await;
            match result {
                Ok(resp) if resp.status().is_success() => {
                    tracing::debug!("observaboard ingest accepted");
                }
                Ok(resp) => {
                    tracing::warn!("observaboard ingest returned {}", resp.status());
                }
                Err(e) => {
                    tracing::warn!("observaboard ingest failed: {e}");
                }
            }
        });
    }

    (StatusCode::CREATED, Json(event)).into_response()
}

// Lists audit events with optional filters; admin role required
pub async fn list_audit_events(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(params): Query<ListAuditEventsQuery>,
) -> Response {
    let claims = match require_auth(&headers) {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if !claims.has_role("admin") {
        return error_response(StatusCode::FORBIDDEN, "FORBIDDEN", "admin role required");
    }

    let limit = params.limit.unwrap_or(50).clamp(1, 200) as i64;
    let offset = params.offset.unwrap_or(0) as i64;

    let mut where_clauses: Vec<String> = Vec::new();
    let mut bind_vals: Vec<String> = Vec::new();
    let mut param_idx = 1usize;

    if let Some(v) = &params.entity_type {
        where_clauses.push(format!("entity_type = ${}", param_idx));
        param_idx += 1;
        bind_vals.push(v.clone());
    }
    if let Some(v) = &params.entity_id {
        where_clauses.push(format!("entity_id = ${}", param_idx));
        param_idx += 1;
        bind_vals.push(v.clone());
    }
    if let Some(v) = &params.actor_id {
        where_clauses.push(format!("actor_id = ${}", param_idx));
        param_idx += 1;
        bind_vals.push(v.clone());
    }
    if let Some(v) = &params.action {
        where_clauses.push(format!("action = ${}", param_idx));
        param_idx += 1;
        bind_vals.push(v.clone());
    }
    if let Some(v) = &params.created_after {
        where_clauses.push(format!("created_at >= ${}", param_idx));
        param_idx += 1;
        bind_vals.push(v.clone());
    }
    if let Some(v) = &params.created_before {
        where_clauses.push(format!("created_at <= ${}", param_idx));
        param_idx += 1;
        bind_vals.push(v.clone());
    }

    let where_stmt = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };

    let rows_sql = format!(
        "SELECT id, entity_type, entity_id, action, actor_id, entity_label, payload, created_at \
         FROM audit_events{} ORDER BY created_at DESC, id DESC LIMIT ${} OFFSET ${}",
        where_stmt,
        param_idx,
        param_idx + 1
    );
    let count_sql = format!("SELECT COUNT(*) FROM audit_events{}", where_stmt);

    let mut rows_query = sqlx::query_as::<_, AuditEvent>(&rows_sql);
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);

    for val in &bind_vals {
        rows_query = rows_query.bind(val);
        count_query = count_query.bind(val);
    }
    rows_query = rows_query.bind(limit).bind(offset);

    let rows = rows_query.fetch_all(&state.pool).await;
    let total = count_query.fetch_one(&state.pool).await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("list_audit_events db error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    };

    let total = match total {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("list_audit_events count error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    };

    tracing::debug!(
        actor = %claims.sub,
        count = rows.len(),
        ?params,
        "list_audit_events ok"
    );

    Json(ListAuditEventsResponse {
        data: rows,
        total,
        limit: limit as u32,
        offset: offset as u32,
    })
    .into_response()
}
