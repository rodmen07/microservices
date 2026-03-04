use std::{collections::HashMap, env, net::SocketAddr, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    documents: Arc<RwLock<HashMap<Uuid, SearchDocument>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SearchDocument {
    id: Uuid,
    entity_type: String,
    entity_id: String,
    title: String,
    body: String,
}

#[derive(Debug, Deserialize)]
struct IndexDocumentRequest {
    entity_type: String,
    entity_id: String,
    title: String,
    body: String,
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
}

#[derive(Debug, Serialize)]
struct SearchResult {
    id: Uuid,
    entity_type: String,
    entity_id: String,
    title: String,
    snippet: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "search_service=info,tower_http=info".into()),
        )
        .init();

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3016);

    let state = AppState {
        documents: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route("/api/v1/search", get(search_documents))
        .route("/api/v1/search/documents", get(list_documents).post(index_document))
        .route(
            "/api/v1/search/documents/:id",
            get(get_document).patch(update_document).delete(delete_document),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid host/port configuration");

    info!("search-service listening on {}", address);

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind listener");

    axum::serve(listener, app)
        .await
        .expect("server failed");
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn list_documents(State(state): State<AppState>) -> Json<Vec<SearchDocument>> {
    let documents = state.documents.read().await;
    Json(documents.values().cloned().collect())
}

async fn get_document(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<SearchDocument>, StatusCode> {
    let documents = state.documents.read().await;
    documents
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn index_document(
    State(state): State<AppState>,
    Json(request): Json<IndexDocumentRequest>,
) -> Result<(StatusCode, Json<SearchDocument>), StatusCode> {
    let entity_type = request.entity_type.trim();
    let entity_id = request.entity_id.trim();
    let title = request.title.trim();
    let body = request.body.trim();

    if entity_type.is_empty() || entity_id.is_empty() || title.is_empty() || body.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let document = SearchDocument {
        id: Uuid::new_v4(),
        entity_type: entity_type.to_string(),
        entity_id: entity_id.to_string(),
        title: title.to_string(),
        body: body.to_string(),
    };

    let mut documents = state.documents.write().await;
    documents.insert(document.id, document.clone());

    Ok((StatusCode::CREATED, Json(document)))
}

async fn update_document(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<IndexDocumentRequest>,
) -> Result<Json<SearchDocument>, StatusCode> {
    let mut documents = state.documents.write().await;
    let existing = documents.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;

    let entity_type = request.entity_type.trim();
    let entity_id = request.entity_id.trim();
    let title = request.title.trim();
    let body = request.body.trim();

    if entity_type.is_empty() || entity_id.is_empty() || title.is_empty() || body.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    existing.entity_type = entity_type.to_string();
    existing.entity_id = entity_id.to_string();
    existing.title = title.to_string();
    existing.body = body.to_string();

    Ok(Json(existing.clone()))
}

async fn delete_document(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<StatusCode, StatusCode> {
    let mut documents = state.documents.write().await;
    documents
        .remove(&id)
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn search_documents(
    Query(query): Query<SearchQuery>,
    State(state): State<AppState>,
) -> Json<Vec<SearchResult>> {
    let term = query.q.trim().to_lowercase();
    if term.is_empty() {
        return Json(Vec::new());
    }

    let documents = state.documents.read().await;
    let mut results = Vec::new();

    for document in documents.values() {
        let haystack = format!("{} {}", document.title, document.body).to_lowercase();
        if haystack.contains(&term) {
            let snippet = if document.body.len() > 140 {
                format!("{}...", &document.body[..140])
            } else {
                document.body.clone()
            };

            results.push(SearchResult {
                id: document.id,
                entity_type: document.entity_type.clone(),
                entity_id: document.entity_id.clone(),
                title: document.title.clone(),
                snippet,
            });
        }
    }

    Json(results)
}
