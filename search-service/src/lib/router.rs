use std::env;

use axum::{routing::get, Router};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::app_state::AppState;
use crate::handlers::{
    documents::{
        delete_document, delete_document_by_entity, get_document, index_document, list_documents,
        search_documents, update_document,
    },
    health::health,
};

// Builds a CORS layer from the ALLOWED_ORIGINS env var, permissive for "*", restrictive for a list, or blocking if unset
pub fn build_cors_layer() -> CorsLayer {
    let origins = env::var("ALLOWED_ORIGINS").unwrap_or_default();
    if origins.trim() == "*" {
        panic!("ALLOWED_ORIGINS=* is not allowed — use an explicit origin list in production");
    }
    if origins.trim().is_empty() {
        return CorsLayer::new();
    }
    let allowed: Vec<_> = origins
        .split(',')
        .filter_map(|o| o.trim().parse().ok())
        .collect();
    CorsLayer::new().allow_origin(allowed)
}

// Assembles the full Axum router with the search query endpoint and all document CRUD routes, CORS, and tracing middleware
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route("/api/v1/search", get(search_documents))
        .route(
            "/api/v1/search/documents",
            get(list_documents).post(index_document),
        )
        .route(
            "/api/v1/search/documents/by-entity/{entity_id}",
            axum::routing::delete(delete_document_by_entity),
        )
        .route(
            "/api/v1/search/documents/{id}",
            get(get_document)
                .patch(update_document)
                .delete(delete_document),
        )
        .with_state(state)
        .layer(build_cors_layer())
        .layer(TraceLayer::new_for_http())
}
