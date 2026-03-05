use std::env;

use axum::{
    Router,
    http::{HeaderValue, Method},
    routing::get,
};
use tower_http::{cors::{Any, CorsLayer}, trace::TraceLayer};

use crate::{
    AppState,
    handlers::{contacts, health},
};

fn build_cors_layer() -> CorsLayer {
    let configured = env::var("ALLOWED_ORIGINS").unwrap_or_default();

    let origins: Vec<HeaderValue> = configured
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| HeaderValue::from_str(s).ok())
        .collect();

    if origins.is_empty() {
        return CorsLayer::new()
            .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE, Method::OPTIONS])
            .allow_headers(Any);
    }

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE, Method::OPTIONS])
        .allow_headers(Any)
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::health))
        .route(
            "/api/v1/contacts",
            get(contacts::list_contacts).post(contacts::create_contact),
        )
        .route(
            "/api/v1/contacts/{id}",
            get(contacts::get_contact)
                .patch(contacts::update_contact)
                .delete(contacts::delete_contact),
        )
        .with_state(state)
        .layer(build_cors_layer())
        .layer(TraceLayer::new_for_http())
}
