use serde::{Deserialize, Serialize};
use sqlx::FromRow;

pub use axum_api_kit::{ApiError, HealthResponse};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Opportunity {
    pub id: String,
    pub owner_id: String,
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
