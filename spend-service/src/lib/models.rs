use serde::{Deserialize, Serialize};
use sqlx::FromRow;

pub use axum_api_kit::{ApiError, HealthResponse};

pub const VALID_PLATFORMS: &[&str] = &["gcp", "flyio", "anthropic", "github_copilot", "github", "aws"];
pub const VALID_GRANULARITIES: &[&str] = &["daily", "monthly"];

/// Provenance values for `SpendRecord.source`, one constant per writer.
///
/// These are the only values the service ever writes: `create_spend` stamps
/// `SOURCE_MANUAL`, and each sync path stamps its own constant. The
/// update/delete record-source guard compares against `SOURCE_MANUAL`.
/// `VALID_SOURCES` is the code-side home of the vocabulary that
/// `openapi.yaml` documents twice (the `SpendRecord.source` schema `enum`
/// and the list `source` filter's "Known values" sentence);
/// `tests/source_vocabulary.rs` is the drift guard that reads both.
pub const SOURCE_MANUAL: &str = "manual";
pub const SOURCE_BIGQUERY: &str = "bigquery";
pub const SOURCE_FLYIO_GRAPHQL: &str = "flyio_graphql";
pub const SOURCE_GITHUB_API: &str = "github_api";
pub const SOURCE_AWS_COST_EXPLORER: &str = "aws_cost_explorer";
pub const VALID_SOURCES: &[&str] = &[
    SOURCE_MANUAL,
    SOURCE_BIGQUERY,
    SOURCE_FLYIO_GRAPHQL,
    SOURCE_GITHUB_API,
    SOURCE_AWS_COST_EXPLORER,
];

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

pub type ListSpendResponse = axum_api_kit::ListResponse<SpendRecord>;

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
