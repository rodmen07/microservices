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
    activities: Arc<RwLock<HashMap<Uuid, Activity>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Activity {
    id: Uuid,
    account_id: Option<Uuid>,
    contact_id: Option<Uuid>,
    activity_type: String,
    subject: String,
    notes: Option<String>,
    due_at: Option<String>,
    completed: bool,
}

#[derive(Debug, Deserialize)]
struct CreateActivityRequest {
    account_id: Option<Uuid>,
    contact_id: Option<Uuid>,
    activity_type: String,
    subject: String,
    notes: Option<String>,
    due_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateActivityRequest {
    activity_type: Option<String>,
    subject: Option<String>,
    notes: Option<String>,
    due_at: Option<String>,
    completed: Option<bool>,
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
                .unwrap_or_else(|_| "activities_service=info,tower_http=info".into()),
        )
        .init();

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3013);

    let state = AppState {
        activities: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route("/api/v1/activities", get(list_activities).post(create_activity))
        .route(
            "/api/v1/activities/:id",
            get(get_activity).patch(update_activity).delete(delete_activity),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid host/port configuration");

    info!("activities-service listening on {}", address);

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

async fn list_activities(State(state): State<AppState>) -> Json<Vec<Activity>> {
    let activities = state.activities.read().await;
    Json(activities.values().cloned().collect())
}

async fn get_activity(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Activity>, StatusCode> {
    let activities = state.activities.read().await;
    activities
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create_activity(
    State(state): State<AppState>,
    Json(request): Json<CreateActivityRequest>,
) -> Result<(StatusCode, Json<Activity>), StatusCode> {
    let activity_type = request.activity_type.trim();
    let subject = request.subject.trim();

    if activity_type.is_empty() || subject.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let activity = Activity {
        id: Uuid::new_v4(),
        account_id: request.account_id,
        contact_id: request.contact_id,
        activity_type: activity_type.to_string(),
        subject: subject.to_string(),
        notes: request
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        due_at: request
            .due_at
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        completed: false,
    };

    let mut activities = state.activities.write().await;
    activities.insert(activity.id, activity.clone());

    Ok((StatusCode::CREATED, Json(activity)))
}

async fn update_activity(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<UpdateActivityRequest>,
) -> Result<Json<Activity>, StatusCode> {
    let mut activities = state.activities.write().await;
    let existing = activities.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;

    if let Some(activity_type) = request.activity_type {
        let normalized = activity_type.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.activity_type = normalized.to_string();
    }

    if let Some(subject) = request.subject {
        let normalized = subject.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.subject = normalized.to_string();
    }

    if let Some(notes) = request.notes {
        let normalized = notes.trim();
        existing.notes = if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_string())
        };
    }

    if let Some(due_at) = request.due_at {
        let normalized = due_at.trim();
        existing.due_at = if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_string())
        };
    }

    if let Some(completed) = request.completed {
        existing.completed = completed;
    }

    Ok(Json(existing.clone()))
}

async fn delete_activity(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<StatusCode, StatusCode> {
    let mut activities = state.activities.write().await;
    activities
        .remove(&id)
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or(StatusCode::NOT_FOUND)
}
