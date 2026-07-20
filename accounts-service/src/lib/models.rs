use serde::{Deserialize, Serialize};
use sqlx::FromRow;

pub use axum_api_kit::{ApiError, HealthResponse};

/// Valid account status values.
pub const VALID_STATUSES: &[&str] = &["active", "inactive", "churned"];

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Account {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub domain: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAccountRequest {
    pub name: String,
    pub domain: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAccountRequest {
    pub name: Option<String>,
    pub domain: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListAccountsQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub status: Option<String>,
    pub q: Option<String>,
    pub owner_id: Option<String>,
}

pub type ListAccountsResponse = axum_api_kit::ListResponse<Account>;
