use std::env;

use axum::{routing::get, Router};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::app_state::AppState;
use crate::handlers::{
    health::health,
    workflows::{create_workflow, delete_workflow, get_workflow, list_workflows, update_workflow},
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

// Assembles the full Axum router with all workflow routes, CORS, and tracing middleware
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route(
            "/api/v1/workflows",
            get(list_workflows).post(create_workflow),
        )
        .route(
            "/api/v1/workflows/{id}",
            get(get_workflow)
                .patch(update_workflow)
                .delete(delete_workflow),
        )
        .with_state(state)
        .layer(build_cors_layer())
        .layer(TraceLayer::new_for_http())
}
