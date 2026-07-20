use serde::{Deserialize, Serialize};
use sqlx::FromRow;

pub use axum_api_kit::{ApiError, HealthResponse};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub trigger_event: String,
    pub action_type: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub trigger_event: String,
    pub action_type: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowRequest {
    pub name: Option<String>,
    pub trigger_event: Option<String>,
    pub action_type: Option<String>,
    pub enabled: Option<bool>,
}
