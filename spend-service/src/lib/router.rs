use std::env;

use axum::{
    http::Method,
    routing::{get, post},
    Router,
};
use tower_http::{cors::{Any, CorsLayer}, trace::TraceLayer};

use crate::app_state::AppState;
use crate::handlers::{
    health::health,
    spend::{
        create_spend, delete_spend, get_spend, get_summary, list_spend, sync_flyio, sync_gcp,
        update_spend,
    },
};

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

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route("/api/v1/spend", get(list_spend).post(create_spend))
        .route("/api/v1/spend/summary", get(get_summary))
        .route("/api/v1/spend/sync/gcp", post(sync_gcp))
        .route("/api/v1/spend/sync/flyio", post(sync_flyio))
        .route(
            "/api/v1/spend/{id}",
            get(get_spend).patch(update_spend).delete(delete_spend),
        )
        .with_state(state)
        .layer(build_cors_layer())
        .layer(TraceLayer::new_for_http())
}
