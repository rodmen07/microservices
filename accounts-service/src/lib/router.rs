use std::env;

use axum::{
    http::{HeaderValue, Method},
    routing::get,
    Router,
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::{
    handlers::{accounts, health},
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
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
}

// Assembles the full Axum router with all account routes, CORS, and tracing middleware.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::health))
        .route(
            "/api/v1/accounts",
            get(accounts::list_accounts).post(accounts::create_account),
        )
        .route(
            "/api/v1/accounts/{id}",
            get(accounts::get_account)
                .patch(accounts::update_account)
                .delete(accounts::delete_account),
        )
        .with_state(state)
        .layer(build_cors_layer())
        .layer(TraceLayer::new_for_http())
}
