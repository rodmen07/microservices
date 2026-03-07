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
pub struct SearchDocument {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub title: String,
    pub snippet: String,
}

#[derive(Debug, Deserialize)]
pub struct IndexDocumentRequest {
    pub entity_type: String,
    pub entity_id: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}
