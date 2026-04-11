use std::env;

use axum::{
    http::{HeaderValue, Method},
    routing::{get, post},
    Router,
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::{
    handlers::{audit_events, health},
    AppState,
};

// Builds a CORS layer from the ALLOWED_ORIGINS env var, falling back to permissive headers if unset.
fn build_cors_layer() -> CorsLayer {
    let configured = env::var("ALLOWED_ORIGINS").unwrap_or_default();

    let origins: Vec<HeaderValue> = configured
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| HeaderValue::from_str(s).ok())
        .collect();

    if origins.is_empty() {
        panic!("ALLOWED_ORIGINS must be set — refusing to start with permissive CORS");
    }

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
}

// Assembles the full Axum router with audit event routes, CORS, and tracing middleware.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::health))
        .route(
            "/api/v1/audit-events",
            post(audit_events::ingest_audit_event).get(audit_events::list_audit_events),
        )
        .with_state(state)
        .layer(build_cors_layer())
        .layer(TraceLayer::new_for_http())
}
