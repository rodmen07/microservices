use std::env;

use axum::{routing::get, Router};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::app_state::AppState;
use crate::handlers::{
    connections::{
        create_connection, delete_connection, get_connection, list_connections, update_connection,
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

// Assembles the full Axum router with all integration connection routes, CORS, and tracing middleware
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route(
            "/api/v1/integrations/connections",
            get(list_connections).post(create_connection),
        )
        .route(
            "/api/v1/integrations/connections/{id}",
            get(get_connection)
                .patch(update_connection)
                .delete(delete_connection),
        )
        .with_state(state)
        .layer(build_cors_layer())
        .layer(TraceLayer::new_for_http())
}
