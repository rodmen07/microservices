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
    auth::{validate_authorization_header, AuthClaims},
    models::{ApiError, IndexDocumentRequest, SearchDocument, SearchQuery, SearchResult},
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
fn require_auth(headers: &HeaderMap) -> Result<AuthClaims, Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

// Searches indexed documents by a query term against title and body, returning truncated snippets
pub async fn search_documents(
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
    State(state): State<AppState>,
) -> Result<Json<Vec<SearchResult>>, Response> {
    let claims = require_auth(&headers)?;

    let term = query.q.trim().to_lowercase();
    if term.is_empty() {
        tracing::debug!(actor = %claims.sub, term = %term, count = 0, "search_documents ok (empty term)");
        return Ok(Json(vec![]));
    }

    let pattern = format!("%{term}%");
    let rows = sqlx::query_as::<_, SearchDocument>(
        "SELECT id, entity_type, entity_id, title, body, created_at, updated_at
         FROM search_documents
         WHERE LOWER(title) LIKE $1 OR LOWER(body) LIKE $2
         ORDER BY created_at DESC",
    )
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "database error searching documents");
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

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
        .collect::<Vec<SearchResult>>();

    tracing::debug!(actor = %claims.sub, term = %term, count = results.len(), "search_documents ok");
    Ok(Json(results))
}

// Returns all indexed search documents ordered by creation date descending
pub async fn list_documents(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<SearchDocument>>, Response> {
    let claims = require_auth(&headers)?;

    let rows = sqlx::query_as::<_, SearchDocument>(
        "SELECT id, entity_type, entity_id, title, body, created_at, updated_at
         FROM search_documents ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "database error listing documents");
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    tracing::debug!(actor = %claims.sub, count = rows.len(), "list_documents ok");
    Ok(Json(rows))
}

// Fetches a single indexed document by ID, returning 404 if it does not exist
pub async fn get_document(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<SearchDocument>, Response> {
    let claims = require_auth(&headers)?;

    let row = sqlx::query_as::<_, SearchDocument>(
        "SELECT id, entity_type, entity_id, title, body, created_at, updated_at
         FROM search_documents WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, document_id = %id, "database error getting document");
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "document not found"))?;

    tracing::debug!(actor = %claims.sub, document_id = %id, "get_document ok");
    Ok(Json(row))
}

// Upserts a search document by entity_id — inserts on first call, updates title/body on subsequent calls
pub async fn index_document(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<IndexDocumentRequest>,
) -> Result<Response, Response> {
    let claims = require_auth(&headers)?;

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

    sqlx::query(
        "INSERT INTO search_documents (id, entity_type, entity_id, title, body, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT(entity_id) DO UPDATE SET
             entity_type = excluded.entity_type,
             title       = excluded.title,
             body        = excluded.body,
             updated_at  = excluded.updated_at",
    )
    .bind(&id)
    .bind(&entity_type)
    .bind(&entity_id)
    .bind(&title)
    .bind(&body)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, entity_id = %entity_id, "database error indexing document");
        error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error")
    })?;

    let upserted = sqlx::query_as::<_, SearchDocument>(
        "SELECT id, entity_type, entity_id, title, body, created_at, updated_at
         FROM search_documents WHERE entity_id = $1",
    )
    .bind(&entity_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, entity_id = %entity_id, "database error fetching upserted document");
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    tracing::info!(actor = %claims.sub, entity_type = %entity_type, entity_id = %entity_id, document_id = %upserted.id, "document indexed");
    Ok((StatusCode::CREATED, Json(upserted)).into_response())
}

// Deletes a search document by the source entity's ID — idempotent, returns 204 whether or not it existed
pub async fn delete_document_by_entity(
    headers: HeaderMap,
    Path(entity_id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    let claims = require_auth(&headers)?;

    sqlx::query("DELETE FROM search_documents WHERE entity_id = $1")
        .bind(&entity_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, entity_id = %entity_id, "database error deleting document by entity");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
        })?;

    tracing::info!(actor = %claims.sub, entity_id = %entity_id, "document deleted by entity_id");
    Ok(StatusCode::NO_CONTENT)
}

// Replaces all fields of an existing search document with the provided values
pub async fn update_document(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<IndexDocumentRequest>,
) -> Result<Json<SearchDocument>, Response> {
    let claims = require_auth(&headers)?;

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

    let existing = sqlx::query_scalar::<_, String>("SELECT id FROM search_documents WHERE id = $1")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, document_id = %id, "database error checking for existing document");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
        })?;

    if existing.is_none() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "document not found",
        ));
    }

    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "UPDATE search_documents SET entity_type = $1, entity_id = $2, title = $3, body = $4, updated_at = $5
         WHERE id = $6",
    )
    .bind(&entity_type)
    .bind(&entity_id)
    .bind(&title)
    .bind(&body)
    .bind(&now)
    .bind(&id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, document_id = %id, "database error updating document");
        error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error")
    })?;

    let updated = sqlx::query_as::<_, SearchDocument>(
        "SELECT id, entity_type, entity_id, title, body, created_at, updated_at
         FROM search_documents WHERE id = $1",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, document_id = %id, "database error fetching updated document");
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    tracing::info!(actor = %claims.sub, document_id = %id, "document updated");
    Ok(Json(updated))
}

// Deletes an indexed document by ID, returning 204 on success or 404 if not found
pub async fn delete_document(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    let claims = require_auth(&headers)?;

    let result = sqlx::query("DELETE FROM search_documents WHERE id = $1")
        .bind(&id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, document_id = %id, "database error deleting document");
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
            "document not found",
        ));
    }

    tracing::info!(actor = %claims.sub, document_id = %id, "document deleted");
    Ok(StatusCode::NO_CONTENT)
}
