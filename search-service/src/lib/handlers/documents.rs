use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::validate_authorization_header,
    models::{ApiError, IndexDocumentRequest, SearchDocument, SearchQuery, SearchResult},
};

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = Json(ApiError {
        code: code.to_string(),
        message: message.to_string(),
        details: None,
    });
    (status, body).into_response()
}

fn require_auth(headers: &HeaderMap) -> Result<(), Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map(|_| ())
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

pub async fn search_documents(
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
    State(state): State<AppState>,
) -> Result<Json<Vec<SearchResult>>, Response> {
    require_auth(&headers)?;

    let term = query.q.trim().to_lowercase();
    if term.is_empty() {
        return Ok(Json(vec![]));
    }

    let pattern = format!("%{term}%");
    let rows = sqlx::query_as!(
        SearchDocument,
        "SELECT id, entity_type, entity_id, title, body, created_at, updated_at
         FROM search_documents
         WHERE LOWER(title) LIKE ? OR LOWER(body) LIKE ?
         ORDER BY created_at DESC",
        pattern,
        pattern
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let results = rows
        .into_iter()
        .map(|doc| {
            let snippet = if doc.body.len() > 140 {
                format!("{}...", &doc.body[..140])
            } else {
                doc.body.clone()
            };
            SearchResult {
                id: doc.id,
                entity_type: doc.entity_type,
                entity_id: doc.entity_id,
                title: doc.title,
                snippet,
            }
        })
        .collect();

    Ok(Json(results))
}

pub async fn list_documents(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<SearchDocument>>, Response> {
    require_auth(&headers)?;

    let rows = sqlx::query_as!(
        SearchDocument,
        "SELECT id, entity_type, entity_id, title, body, created_at, updated_at
         FROM search_documents ORDER BY created_at DESC"
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    Ok(Json(rows))
}

pub async fn get_document(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<SearchDocument>, Response> {
    require_auth(&headers)?;

    let row = sqlx::query_as!(
        SearchDocument,
        "SELECT id, entity_type, entity_id, title, body, created_at, updated_at
         FROM search_documents WHERE id = ?",
        id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "document not found"))?;

    Ok(Json(row))
}

pub async fn index_document(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<IndexDocumentRequest>,
) -> Result<Response, Response> {
    require_auth(&headers)?;

    let entity_type = req.entity_type.trim().to_string();
    let entity_id = req.entity_id.trim().to_string();
    let title = req.title.trim().to_string();
    let body = req.body.trim().to_string();

    if entity_type.is_empty() || entity_id.is_empty() || title.is_empty() || body.is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "entity_type, entity_id, title, and body are required",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query!(
        "INSERT INTO search_documents (id, entity_type, entity_id, title, body, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        id,
        entity_type,
        entity_id,
        title,
        body,
        now,
        now,
    )
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let created = sqlx::query_as!(
        SearchDocument,
        "SELECT id, entity_type, entity_id, title, body, created_at, updated_at
         FROM search_documents WHERE id = ?",
        id
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    Ok((StatusCode::CREATED, Json(created)).into_response())
}

pub async fn update_document(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<IndexDocumentRequest>,
) -> Result<Json<SearchDocument>, Response> {
    require_auth(&headers)?;

    let entity_type = req.entity_type.trim().to_string();
    let entity_id = req.entity_id.trim().to_string();
    let title = req.title.trim().to_string();
    let body = req.body.trim().to_string();

    if entity_type.is_empty() || entity_id.is_empty() || title.is_empty() || body.is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "entity_type, entity_id, title, and body are required",
        ));
    }

    let existing = sqlx::query!("SELECT id FROM search_documents WHERE id = ?", id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    if existing.is_none() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "document not found",
        ));
    }

    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query!(
        "UPDATE search_documents SET entity_type = ?, entity_id = ?, title = ?, body = ?, updated_at = ?
         WHERE id = ?",
        entity_type,
        entity_id,
        title,
        body,
        now,
        id
    )
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let updated = sqlx::query_as!(
        SearchDocument,
        "SELECT id, entity_type, entity_id, title, body, created_at, updated_at
         FROM search_documents WHERE id = ?",
        id
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    Ok(Json(updated))
}

pub async fn delete_document(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    require_auth(&headers)?;

    let result = sqlx::query!("DELETE FROM search_documents WHERE id = ?", id)
        .execute(&state.pool)
        .await
        .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    if result.rows_affected() == 0 {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "document not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
