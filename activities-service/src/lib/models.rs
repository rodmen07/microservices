use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Activity {
    pub id: String,
    pub owner_id: String,
    pub account_id: Option<String>,
    pub contact_id: Option<String>,
    pub activity_type: String,
    pub subject: String,
    pub notes: Option<String>,
    pub due_at: Option<String>,
    pub completed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateActivityRequest {
    pub account_id: Option<String>,
    pub contact_id: Option<String>,
    pub activity_type: String,
    pub subject: String,
    pub notes: Option<String>,
    pub due_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateActivityRequest {
    pub activity_type: Option<String>,
    pub subject: Option<String>,
    pub notes: Option<String>,
    pub due_at: Option<String>,
    pub completed: Option<bool>,
}
