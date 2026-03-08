use std::env;

use axum::{routing::get, Router};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::app_state::AppState;
use crate::handlers::{
    health::health,
    opportunities::{
        create_opportunity, delete_opportunity, get_opportunity, list_opportunities,
        update_opportunity,
    },
};

// Builds a CORS layer from the ALLOWED_ORIGINS env var, permissive for "*", restrictive for a list, or blocking if unset
pub fn build_cors_layer() -> CorsLayer {
    let origins = env::var("ALLOWED_ORIGINS").unwrap_or_default();
    if origins.trim() == "*" {
        tracing::warn!("CORS is fully permissive — restrict ALLOWED_ORIGINS in production");
        return CorsLayer::permissive();
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

// Assembles the full Axum router with all opportunity routes, CORS, and tracing middleware
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route(
            "/api/v1/opportunities",
            get(list_opportunities).post(create_opportunity),
        )
        .route(
            "/api/v1/opportunities/{id}",
            get(get_opportunity)
                .patch(update_opportunity)
                .delete(delete_opportunity),
        )
        .with_state(state)
        .layer(build_cors_layer())
        .layer(TraceLayer::new_for_http())
}
