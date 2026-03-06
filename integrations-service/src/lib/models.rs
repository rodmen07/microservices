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
pub struct IntegrationConnection {
    pub id: String,
    pub provider: String,
    pub account_ref: String,
    pub status: String,
    pub last_synced_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateConnectionRequest {
    pub provider: String,
    pub account_ref: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConnectionRequest {
    pub status: Option<String>,
    pub last_synced_at: Option<String>,
}
