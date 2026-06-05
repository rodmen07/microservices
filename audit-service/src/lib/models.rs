use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;

pub use axum_api_kit::ApiError;

pub const VALID_ENTITY_TYPES: &[&str] = &["account", "contact", "opportunity", "activity"];
pub const VALID_ACTIONS: &[&str] = &["created", "updated", "deleted"];

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuditEvent {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub action: String,
    pub actor_id: String,
    pub entity_label: Option<String>,
    pub payload: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAuditEventRequest {
    pub entity_type: String,
    pub entity_id: String,
    pub action: String,
    pub actor_id: String,
    pub entity_label: Option<String>,
    pub payload: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListAuditEventsQuery {
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub actor_id: Option<String>,
    pub action: Option<String>,
    pub created_after: Option<String>,
    pub created_before: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub type ListAuditEventsResponse = axum_api_kit::ListResponse<AuditEvent>;
