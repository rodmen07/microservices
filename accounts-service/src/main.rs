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
    accounts: Arc<RwLock<HashMap<Uuid, Account>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Account {
    id: Uuid,
    name: String,
    domain: Option<String>,
    status: String,
}

#[derive(Debug, Deserialize)]
struct CreateAccountRequest {
    name: String,
    domain: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateAccountRequest {
    name: Option<String>,
    domain: Option<String>,
    status: Option<String>,
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
                .unwrap_or_else(|_| "accounts_service=info,tower_http=info".into()),
        )
        .init();

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3010);

    let state = AppState {
        accounts: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route("/api/v1/accounts", get(list_accounts).post(create_account))
        .route(
            "/api/v1/accounts/:id",
            get(get_account).patch(update_account).delete(delete_account),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid host/port configuration");

    info!("accounts-service listening on {}", address);

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

async fn list_accounts(State(state): State<AppState>) -> Json<Vec<Account>> {
    let accounts = state.accounts.read().await;
    Json(accounts.values().cloned().collect())
}

async fn get_account(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Account>, StatusCode> {
    let accounts = state.accounts.read().await;
    accounts
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create_account(
    State(state): State<AppState>,
    Json(request): Json<CreateAccountRequest>,
) -> Result<(StatusCode, Json<Account>), StatusCode> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let account = Account {
        id: Uuid::new_v4(),
        name: name.to_string(),
        domain: request
            .domain
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        status: "active".to_string(),
    };

    let mut accounts = state.accounts.write().await;
    accounts.insert(account.id, account.clone());

    Ok((StatusCode::CREATED, Json(account)))
}

async fn update_account(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<UpdateAccountRequest>,
) -> Result<Json<Account>, StatusCode> {
    let mut accounts = state.accounts.write().await;
    let existing = accounts.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;

    if let Some(name) = request.name {
        let normalized = name.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.name = normalized.to_string();
    }

    if let Some(domain) = request.domain {
        let normalized = domain.trim();
        existing.domain = if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_string())
        };
    }

    if let Some(status) = request.status {
        let normalized = status.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.status = normalized.to_string();
    }

    Ok(Json(existing.clone()))
}

async fn delete_account(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<StatusCode, StatusCode> {
    let mut accounts = state.accounts.write().await;
    accounts
        .remove(&id)
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or(StatusCode::NOT_FOUND)
}
