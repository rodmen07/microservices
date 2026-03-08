use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;

/// Valid account status values.
pub const VALID_STATUSES: &[&str] = &["active", "inactive", "churned"];

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Account {
    pub id: String,
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
}

#[derive(Debug, Serialize)]
pub struct ListAccountsResponse {
    pub data: Vec<Account>,
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
