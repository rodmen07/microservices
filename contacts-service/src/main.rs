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
    contacts: Arc<RwLock<HashMap<Uuid, Contact>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Contact {
    id: Uuid,
    account_id: Option<Uuid>,
    first_name: String,
    last_name: String,
    email: Option<String>,
    phone: Option<String>,
    lifecycle_stage: String,
}

#[derive(Debug, Deserialize)]
struct CreateContactRequest {
    account_id: Option<Uuid>,
    first_name: String,
    last_name: String,
    email: Option<String>,
    phone: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateContactRequest {
    account_id: Option<Uuid>,
    first_name: Option<String>,
    last_name: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    lifecycle_stage: Option<String>,
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
                .unwrap_or_else(|_| "contacts_service=info,tower_http=info".into()),
        )
        .init();

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3011);

    let state = AppState {
        contacts: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route("/api/v1/contacts", get(list_contacts).post(create_contact))
        .route(
            "/api/v1/contacts/:id",
            get(get_contact).patch(update_contact).delete(delete_contact),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid host/port configuration");

    info!("contacts-service listening on {}", address);

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

async fn list_contacts(State(state): State<AppState>) -> Json<Vec<Contact>> {
    let contacts = state.contacts.read().await;
    Json(contacts.values().cloned().collect())
}

async fn get_contact(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Contact>, StatusCode> {
    let contacts = state.contacts.read().await;
    contacts
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create_contact(
    State(state): State<AppState>,
    Json(request): Json<CreateContactRequest>,
) -> Result<(StatusCode, Json<Contact>), StatusCode> {
    let first_name = request.first_name.trim();
    let last_name = request.last_name.trim();

    if first_name.is_empty() || last_name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let contact = Contact {
        id: Uuid::new_v4(),
        account_id: request.account_id,
        first_name: first_name.to_string(),
        last_name: last_name.to_string(),
        email: request
            .email
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        phone: request
            .phone
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        lifecycle_stage: "lead".to_string(),
    };

    let mut contacts = state.contacts.write().await;
    contacts.insert(contact.id, contact.clone());

    Ok((StatusCode::CREATED, Json(contact)))
}

async fn update_contact(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<UpdateContactRequest>,
) -> Result<Json<Contact>, StatusCode> {
    let mut contacts = state.contacts.write().await;
    let existing = contacts.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;

    if let Some(account_id) = request.account_id {
        existing.account_id = Some(account_id);
    }

    if let Some(first_name) = request.first_name {
        let normalized = first_name.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.first_name = normalized.to_string();
    }

    if let Some(last_name) = request.last_name {
        let normalized = last_name.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.last_name = normalized.to_string();
    }

    if let Some(email) = request.email {
        let normalized = email.trim();
        existing.email = if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_string())
        };
    }

    if let Some(phone) = request.phone {
        let normalized = phone.trim();
        existing.phone = if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_string())
        };
    }

    if let Some(lifecycle_stage) = request.lifecycle_stage {
        let normalized = lifecycle_stage.trim();
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        existing.lifecycle_stage = normalized.to_string();
    }

    Ok(Json(existing.clone()))
}

async fn delete_contact(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<StatusCode, StatusCode> {
    let mut contacts = state.contacts.write().await;
    contacts
        .remove(&id)
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or(StatusCode::NOT_FOUND)
}
