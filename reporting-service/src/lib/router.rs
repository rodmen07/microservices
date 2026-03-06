use std::env;

use axum::{Router, routing::get};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::app_state::AppState;
use crate::handlers::{
    reports::{
        create_report, delete_report, get_dashboard_summary, get_report, list_reports,
        update_report,
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
        .route("/api/v1/reports/dashboard", get(get_dashboard_summary))
        .route("/api/v1/reports", get(list_reports).post(create_report))
        .route(
            "/api/v1/reports/{id}",
            get(get_report).patch(update_report).delete(delete_report),
        )
        .with_state(state)
        .layer(build_cors_layer())
        .layer(TraceLayer::new_for_http())
}
