use crate::{app_state::AppState, models::HealthResponse};
use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;

// Performs a live database ping to check service health
pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({ "status": HealthResponse { status: "ok" }.status })),
        ),
        Err(e) => {
            tracing::error!(error = %e, "health check db ping failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "degraded", "error": e.to_string() })),
            )
        }
    }
}
