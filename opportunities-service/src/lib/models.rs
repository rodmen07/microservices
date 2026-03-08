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
pub struct Opportunity {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub stage: String,
    pub amount: f64,
    pub close_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateOpportunityRequest {
    pub account_id: String,
    pub name: String,
    pub stage: Option<String>,
    pub amount: Option<f64>,
    pub close_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateOpportunityRequest {
    pub name: Option<String>,
    pub stage: Option<String>,
    pub amount: Option<f64>,
    pub close_date: Option<String>,
}
