use std::{collections::HashMap, env, net::SocketAddr, sync::Arc};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    workflows: Arc<RwLock<HashMap<Uuid, Workflow>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Workflow {
    id: Uuid,
    name: String,
    trigger_event: String,
    action_type: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct CreateWorkflowRequest {
    name: String,
    trigger_event: String,
    action_type: String,
}

#[derive(Debug, Deserialize)]
struct UpdateWorkflowRequest {
    name: Option<String>,
    trigger_event: Option<String>,
    action_type: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "automation_service=info,tower_http=info".into()),
        )
        .init();

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3014);

    let state = AppState {
        workflows: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route("/api/v1/workflows", get(list_workflows).post(create_workflow))
        .route(
            "/api/v1/workflows/:id",
            get(get_workflow)
                .patch(update_workflow)
                .delete(delete_workflow),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid host/port configuration");

    info!("automation-service listening on {}", address);

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind listener");

    axum::serve(listener, app)
        .await
        .expect("server failed");
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn list_workflows(State(state): State<AppState>) -> Json<Vec<Workflow>> {
    let workflows = state.workflows.read().await;
    Json(workflows.values().cloned().collect())
}

async fn get_workflow(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Workflow>, StatusCode> {
    let workflows = state.workflows.read().await;
    workflows
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create_workflow(
    State(state): State<AppState>,
    Json(request): Json<CreateWorkflowRequest>,
) -> Result<(StatusCode, Json<Workflow>), StatusCode> {
    let name = request.name.trim();
    let trigger_event = request.trigger_event.trim();
    let action_type = request.action_type.trim();

    if name.is_empty() || trigger_event.is_empty() || action_type.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let workflow = Workflow {
        id: Uuid::new_v4(),
        name: name.to_string(),
        trigger_event: trigger_event.to_string(),
        action_type: action_type.to_string(),
        enabled: true,
    };

    let mut workflows = state.workflows.write().await;
    workflows.insert(workflow.id, workflow.clone());

    Ok((StatusCode::CREATED, Json(workflow)))
}

async fn update_workflow(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<UpdateWorkflowRequest>,
) -> Result<Json<Workflow>, StatusCode> {
    let mut workflows = state.workflows.write().await;
    let existing = workflows.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;

    if let Some(name) = request.name {
        let normalized = name.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.name = normalized.to_string();
    }

    if let Some(trigger_event) = request.trigger_event {
        let normalized = trigger_event.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.trigger_event = normalized.to_string();
    }

    if let Some(action_type) = request.action_type {
        let normalized = action_type.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.action_type = normalized.to_string();
    }

    if let Some(enabled) = request.enabled {
        existing.enabled = enabled;
    }

    Ok(Json(existing.clone()))
}

async fn delete_workflow(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<StatusCode, StatusCode> {
    let mut workflows = state.workflows.write().await;
    workflows
        .remove(&id)
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or(StatusCode::NOT_FOUND)
}
