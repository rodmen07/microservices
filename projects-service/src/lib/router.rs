use std::env;

use axum::{
    routing::{get, patch},
    Router,
};
use axum::http::Method;
use tower_http::{cors::{Any, CorsLayer}, trace::TraceLayer};

use crate::app_state::AppState;
use crate::handlers::{
    deliverables::{create_deliverable, delete_deliverable, list_deliverables, update_deliverable},
    health::health,
    messages::{create_message, list_messages},
    milestones::{create_milestone, delete_milestone, list_milestones, update_milestone},
    projects::{create_project, delete_project, get_project, list_projects, update_project},
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
        // Projects
        .route("/api/v1/projects", get(list_projects).post(create_project))
        .route(
            "/api/v1/projects/{id}",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
        // Milestones
        .route(
            "/api/v1/projects/{project_id}/milestones",
            get(list_milestones).post(create_milestone),
        )
        .route(
            "/api/v1/milestones/{id}",
            patch(update_milestone).delete(delete_milestone),
        )
        // Deliverables
        .route(
            "/api/v1/milestones/{milestone_id}/deliverables",
            get(list_deliverables).post(create_deliverable),
        )
        .route(
            "/api/v1/deliverables/{id}",
            patch(update_deliverable).delete(delete_deliverable),
        )
        // Messages
        .route(
            "/api/v1/projects/{project_id}/messages",
            get(list_messages).post(create_message),
        )
        .with_state(state)
        .layer(build_cors_layer())
        .layer(TraceLayer::new_for_http())
}
