use std::env;

use axum::{routing::get, Router};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::app_state::AppState;
use crate::handlers::{
    activities::{
        create_activity, delete_activity, get_activity, list_activities, update_activity,
    },
    health::health,
};

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
