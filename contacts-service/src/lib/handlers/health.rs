use axum::Json;

use crate::models::HealthResponse;

// Returns a static JSON health check response indicating the service is up
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}
