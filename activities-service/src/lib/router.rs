use std::env;

use axum::{http::Method, routing::get, Router};
use tower_http::{cors::{Any, CorsLayer}, trace::TraceLayer};

use crate::app_state::AppState;
use crate::handlers::{
    activities::{
        create_activity, delete_activity, get_activity, list_activities, update_activity,
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
    CorsLayer::new()
        .allow_origin(allowed)
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE, Method::OPTIONS])
        .allow_headers(Any)
}

// Assembles the full Axum router with all activity routes, CORS, and tracing middleware
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route(
            "/api/v1/activities",
            get(list_activities).post(create_activity),
        )
        .route(
            "/api/v1/activities/{id}",
            get(get_activity)
                .patch(update_activity)
                .delete(delete_activity),
        )
        .with_state(state)
        .layer(build_cors_layer())
        .layer(TraceLayer::new_for_http())
}
