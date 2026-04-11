use std::env;

use axum::{http::Method, routing::get, Router};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::app_state::AppState;
use crate::handlers::{
    health::health,
    reports::{
        create_report, delete_report, export_reports, get_dashboard, get_dashboard_summary,
        get_report, list_reports, update_report,
    },
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
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
}

// Assembles the full Axum router with all report routes and the dashboard endpoint, CORS, and tracing middleware
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route("/api/v1/dashboard", get(get_dashboard))
        .route("/api/v1/reports/dashboard", get(get_dashboard_summary))
        .route("/api/v1/reports", get(list_reports).post(create_report))
        .route("/api/v1/reports/export", get(export_reports))
        .route(
            "/api/v1/reports/{id}",
            get(get_report).patch(update_report).delete(delete_report),
        )
        .with_state(state)
        .layer(build_cors_layer())
        .layer(TraceLayer::new_for_http())
}
