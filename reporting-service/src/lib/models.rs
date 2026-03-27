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
pub struct SavedReport {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub metric: String,
    pub dimension: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardSummary {
    pub active_reports: i64,
    pub core_metrics: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DashboardView {
    pub accounts: Option<i64>,
    pub contacts: Option<i64>,
    pub opportunities: Option<i64>,
    pub activities: Option<i64>,
    pub reports: i64,
    pub core_metrics: Vec<String>,
    pub stage_distribution: Option<std::collections::HashMap<String, i64>>,
    pub recent_activities: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateReportRequest {
    pub name: String,
    pub description: Option<String>,
    pub metric: String,
    pub dimension: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateReportRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub metric: Option<String>,
    pub dimension: Option<String>,
}
