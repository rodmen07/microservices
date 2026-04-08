use serde::{Deserialize, Serialize};
use sqlx::FromRow;

pub const VALID_PLATFORMS: &[&str] = &["gcp", "flyio", "anthropic", "github_copilot"];
pub const VALID_GRANULARITIES: &[&str] = &["daily", "monthly"];
pub const VALID_SOURCES: &[&str] = &["manual", "bigquery", "flyio_graphql"];

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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpendRecord {
    pub id: String,
    pub platform: String,
    pub date: String,
    pub amount_usd: f64,
    pub granularity: String,
    pub service_label: Option<String>,
    pub source: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSpendRequest {
    pub platform: String,
    pub date: String,
    pub amount_usd: f64,
    pub granularity: Option<String>,
    pub service_label: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSpendRequest {
    pub platform: Option<String>,
    pub date: Option<String>,
    pub amount_usd: Option<f64>,
    pub granularity: Option<String>,
    pub service_label: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListSpendQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub platform: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct SummaryQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListSpendResponse {
    pub data: Vec<SpendRecord>,
    pub total: i64,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Serialize)]
pub struct SpendSummary {
    pub total_usd: f64,
    pub by_platform: Vec<PlatformTotal>,
    pub by_month: Vec<MonthTotal>,
}

#[derive(Debug, Serialize)]
pub struct PlatformTotal {
    pub platform: String,
    pub total_usd: f64,
}

#[derive(Debug, Serialize)]
pub struct MonthTotal {
    pub month: String,
    pub total_usd: f64,
}

#[derive(Debug, Serialize)]
pub struct SyncResult {
    pub platform: String,
    pub records_imported: usize,
    pub records_skipped: usize,
    pub errors: Vec<String>,
}
