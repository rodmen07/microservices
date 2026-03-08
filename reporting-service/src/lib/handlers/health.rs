use crate::models::HealthResponse;
use axum::Json;

// Returns a static JSON health check response indicating the service is up
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}
