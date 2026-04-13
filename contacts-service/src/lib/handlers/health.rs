use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;

use crate::AppState;

// Returns a static JSON health check response indicating the service is up
pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({"status": "ok"})),
        ),
        Err(e) => {
            tracing::error!(error = %e, "health check db ping failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"status": "degraded", "error": e.to_string()})),
            )
        }
    }
}
