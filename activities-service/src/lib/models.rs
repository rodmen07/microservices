use serde::{Deserialize, Serialize};
use sqlx::FromRow;

pub use axum_api_kit::ApiError;

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
