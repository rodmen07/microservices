use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;

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

#[derive(Debug, Serialize)]
pub struct ListAuditEventsResponse {
    pub data: Vec<AuditEvent>,
    pub total: i64,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}
