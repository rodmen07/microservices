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

// --- Projects ---

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Project {
    pub id: String,
    pub account_id: String,
    pub client_user_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub start_date: Option<String>,
    pub target_end_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub account_id: String,
    pub client_user_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub status: Option<String>,
    pub start_date: Option<String>,
    pub target_end_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    pub client_user_id: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub start_date: Option<String>,
    pub target_end_date: Option<String>,
}

// --- Milestones ---

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Milestone {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub description: Option<String>,
    pub due_date: Option<String>,
    pub status: String,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateMilestoneRequest {
    pub name: String,
    pub description: Option<String>,
    pub due_date: Option<String>,
    pub status: Option<String>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMilestoneRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub due_date: Option<String>,
    pub status: Option<String>,
    pub sort_order: Option<i64>,
}

// --- Deliverables ---

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Deliverable {
    pub id: String,
    pub milestone_id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub estimated_hours: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateDeliverableRequest {
    pub name: String,
    pub description: Option<String>,
    pub status: Option<String>,
    pub estimated_hours: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDeliverableRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub estimated_hours: Option<f64>,
}

// --- Project Links ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectLink {
    pub id: String,
    pub project_id: String,
    pub link_type: String,
    pub label: String,
    pub url: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectLinkRequest {
    pub link_type: String,
    pub label: String,
    pub url: String,
    pub sort_order: Option<i64>,
}

// --- Project Emails ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectEmail {
    pub id: String,
    pub project_id: String,
    pub thread_id: String,
    pub subject: String,
    pub from_email: String,
    pub snippet: Option<String>,
    pub body_html: Option<String>,
    pub received_at: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct UpsertEmailRequest {
    pub thread_id: String,
    pub subject: String,
    pub from_email: String,
    pub snippet: Option<String>,
    pub body_html: Option<String>,
    pub received_at: String,
}

#[derive(Debug, Deserialize)]
pub struct SyncEmailsRequest {
    pub emails: Vec<UpsertEmailRequest>,
}

// --- Messages ---

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Message {
    pub id: String,
    pub project_id: String,
    pub author_id: String,
    pub author_role: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateMessageRequest {
    pub body: String,
}
