use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;

/// Valid lifecycle stage values.
pub const VALID_LIFECYCLE_STAGES: &[&str] =
    &["lead", "prospect", "customer", "churned", "evangelist"];

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Contact {
    pub id: String,
    pub account_id: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub lifecycle_stage: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateContactRequest {
    pub account_id: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub lifecycle_stage: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateContactRequest {
    pub account_id: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub lifecycle_stage: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListContactsQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub account_id: Option<String>,
    pub lifecycle_stage: Option<String>,
    pub q: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListContactsResponse {
    pub data: Vec<Contact>,
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
