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
    opportunities: Arc<RwLock<HashMap<Uuid, Opportunity>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Opportunity {
    id: Uuid,
    account_id: Uuid,
    name: String,
    stage: String,
    amount: f64,
    close_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateOpportunityRequest {
    account_id: Uuid,
    name: String,
    stage: Option<String>,
    amount: f64,
    close_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateOpportunityRequest {
    name: Option<String>,
    stage: Option<String>,
    amount: Option<f64>,
    close_date: Option<String>,
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
                .unwrap_or_else(|_| "opportunities_service=info,tower_http=info".into()),
        )
        .init();

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3012);

    let state = AppState {
        opportunities: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route(
            "/api/v1/opportunities",
            get(list_opportunities).post(create_opportunity),
        )
        .route(
            "/api/v1/opportunities/:id",
            get(get_opportunity)
                .patch(update_opportunity)
                .delete(delete_opportunity),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid host/port configuration");

    info!("opportunities-service listening on {}", address);

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

async fn list_opportunities(State(state): State<AppState>) -> Json<Vec<Opportunity>> {
    let opportunities = state.opportunities.read().await;
    Json(opportunities.values().cloned().collect())
}

async fn get_opportunity(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Opportunity>, StatusCode> {
    let opportunities = state.opportunities.read().await;
    opportunities
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create_opportunity(
    State(state): State<AppState>,
    Json(request): Json<CreateOpportunityRequest>,
) -> Result<(StatusCode, Json<Opportunity>), StatusCode> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    if request.amount < 0.0 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let opportunity = Opportunity {
        id: Uuid::new_v4(),
        account_id: request.account_id,
        name: name.to_string(),
        stage: request
            .stage
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("qualification")
            .to_string(),
        amount: request.amount,
        close_date: request
            .close_date
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    };

    let mut opportunities = state.opportunities.write().await;
    opportunities.insert(opportunity.id, opportunity.clone());

    Ok((StatusCode::CREATED, Json(opportunity)))
}

async fn update_opportunity(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<UpdateOpportunityRequest>,
) -> Result<Json<Opportunity>, StatusCode> {
    let mut opportunities = state.opportunities.write().await;
    let existing = opportunities.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;

    if let Some(name) = request.name {
        let normalized = name.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.name = normalized.to_string();
    }

    if let Some(stage) = request.stage {
        let normalized = stage.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.stage = normalized.to_string();
    }

    if let Some(amount) = request.amount {
        if amount < 0.0 {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.amount = amount;
    }

    if let Some(close_date) = request.close_date {
        let normalized = close_date.trim();
        existing.close_date = if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_string())
        };
    }

    Ok(Json(existing.clone()))
}

async fn delete_opportunity(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<StatusCode, StatusCode> {
    let mut opportunities = state.opportunities.write().await;
    opportunities
        .remove(&id)
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or(StatusCode::NOT_FOUND)
}
