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
    reports: Arc<RwLock<HashMap<Uuid, SavedReport>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedReport {
    id: Uuid,
    name: String,
    description: Option<String>,
    metric: String,
    dimension: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateReportRequest {
    name: String,
    description: Option<String>,
    metric: String,
    dimension: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateReportRequest {
    name: Option<String>,
    description: Option<String>,
    metric: Option<String>,
    dimension: Option<String>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct DashboardSummary {
    active_reports: usize,
    core_metrics: Vec<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "reporting_service=info,tower_http=info".into()),
        )
        .init();

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3017);

    let state = AppState {
        reports: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route("/api/v1/reports/dashboard", get(get_dashboard_summary))
        .route("/api/v1/reports", get(list_reports).post(create_report))
        .route(
            "/api/v1/reports/:id",
            get(get_report).patch(update_report).delete(delete_report),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid host/port configuration");

    info!("reporting-service listening on {}", address);

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

async fn get_dashboard_summary(State(state): State<AppState>) -> Json<DashboardSummary> {
    let reports = state.reports.read().await;
    let mut metrics = reports
        .values()
        .map(|report| report.metric.clone())
        .collect::<Vec<_>>();
    metrics.sort();
    metrics.dedup();

    Json(DashboardSummary {
        active_reports: reports.len(),
        core_metrics: metrics,
    })
}

async fn list_reports(State(state): State<AppState>) -> Json<Vec<SavedReport>> {
    let reports = state.reports.read().await;
    Json(reports.values().cloned().collect())
}

async fn get_report(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<SavedReport>, StatusCode> {
    let reports = state.reports.read().await;
    reports
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create_report(
    State(state): State<AppState>,
    Json(request): Json<CreateReportRequest>,
) -> Result<(StatusCode, Json<SavedReport>), StatusCode> {
    let name = request.name.trim();
    let metric = request.metric.trim();

    if name.is_empty() || metric.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let report = SavedReport {
        id: Uuid::new_v4(),
        name: name.to_string(),
        description: request
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        metric: metric.to_string(),
        dimension: request
            .dimension
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    };

    let mut reports = state.reports.write().await;
    reports.insert(report.id, report.clone());

    Ok((StatusCode::CREATED, Json(report)))
}

async fn update_report(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<UpdateReportRequest>,
) -> Result<Json<SavedReport>, StatusCode> {
    let mut reports = state.reports.write().await;
    let existing = reports.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;

    if let Some(name) = request.name {
        let normalized = name.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.name = normalized.to_string();
    }

    if let Some(description) = request.description {
        let normalized = description.trim();
        existing.description = if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_string())
        };
    }

    if let Some(metric) = request.metric {
        let normalized = metric.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.metric = normalized.to_string();
    }

    if let Some(dimension) = request.dimension {
        let normalized = dimension.trim();
        existing.dimension = if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_string())
        };
    }

    Ok(Json(existing.clone()))
}

async fn delete_report(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<StatusCode, StatusCode> {
    let mut reports = state.reports.write().await;
    reports
        .remove(&id)
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or(StatusCode::NOT_FOUND)
}
