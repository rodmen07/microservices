use serde::{Deserialize, Serialize};
use sqlx::FromRow;

pub use axum_api_kit::ApiError;

/// Valid lifecycle stage values.
pub const VALID_LIFECYCLE_STAGES: &[&str] =
    &["lead", "prospect", "customer", "churned", "evangelist"];

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Contact {
    pub id: String,
    pub owner_id: String,
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
    pub owner_id: Option<String>,
}

pub type ListContactsResponse = axum_api_kit::ListResponse<Contact>;
