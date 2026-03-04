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
    connections: Arc<RwLock<HashMap<Uuid, IntegrationConnection>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IntegrationConnection {
    id: Uuid,
    provider: String,
    account_ref: String,
    status: String,
    last_synced_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateConnectionRequest {
    provider: String,
    account_ref: String,
}

#[derive(Debug, Deserialize)]
struct UpdateConnectionRequest {
    status: Option<String>,
    last_synced_at: Option<String>,
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
                .unwrap_or_else(|_| "integrations_service=info,tower_http=info".into()),
        )
        .init();

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3015);

    let state = AppState {
        connections: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route(
            "/api/v1/integrations/connections",
            get(list_connections).post(create_connection),
        )
        .route(
            "/api/v1/integrations/connections/:id",
            get(get_connection)
                .patch(update_connection)
                .delete(delete_connection),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid host/port configuration");

    info!("integrations-service listening on {}", address);

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

async fn list_connections(State(state): State<AppState>) -> Json<Vec<IntegrationConnection>> {
    let connections = state.connections.read().await;
    Json(connections.values().cloned().collect())
}

async fn get_connection(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<IntegrationConnection>, StatusCode> {
    let connections = state.connections.read().await;
    connections
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create_connection(
    State(state): State<AppState>,
    Json(request): Json<CreateConnectionRequest>,
) -> Result<(StatusCode, Json<IntegrationConnection>), StatusCode> {
    let provider = request.provider.trim();
    let account_ref = request.account_ref.trim();

    if provider.is_empty() || account_ref.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let connection = IntegrationConnection {
        id: Uuid::new_v4(),
        provider: provider.to_string(),
        account_ref: account_ref.to_string(),
        status: "connected".to_string(),
        last_synced_at: None,
    };

    let mut connections = state.connections.write().await;
    connections.insert(connection.id, connection.clone());

    Ok((StatusCode::CREATED, Json(connection)))
}

async fn update_connection(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<UpdateConnectionRequest>,
) -> Result<Json<IntegrationConnection>, StatusCode> {
    let mut connections = state.connections.write().await;
    let existing = connections.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;

    if let Some(status) = request.status {
        let normalized = status.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.status = normalized.to_string();
    }

    if let Some(last_synced_at) = request.last_synced_at {
        let normalized = last_synced_at.trim();
        existing.last_synced_at = if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_string())
        };
    }

    Ok(Json(existing.clone()))
}

async fn delete_connection(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<StatusCode, StatusCode> {
    let mut connections = state.connections.write().await;
    connections
        .remove(&id)
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or(StatusCode::NOT_FOUND)
}
