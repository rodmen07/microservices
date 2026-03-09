output "cloud_sql_connection_name" {
  description = "Cloud SQL connection name for use with Cloud SQL Auth Proxy"
  value       = google_sql_database_instance.main.connection_name
}

output "cloud_sql_public_ip" {
  description = "Cloud SQL public IP address"
  value       = google_sql_database_instance.main.public_ip_address
}

output "artifact_registry_url" {
  description = "Base URL for Docker image pushes"
  value       = "${var.region}-docker.pkg.dev/${var.project_id}/microservices"
}

output "rust_service_urls" {
  description = "Cloud Run URLs for all Rust workspace services"
  value = {
    for k, v in google_cloud_run_v2_service.rust_services : k => v.uri
  }
}

output "task_api_url" {
  description = "Cloud Run URL for backend-service (task-api)"
  value       = google_cloud_run_v2_service.task_api.uri
}

output "ai_orchestrator_url" {
  description = "Cloud Run URL for ai-orchestrator-service"
  value       = google_cloud_run_v2_service.ai_orchestrator.uri
}

output "cloud_run_service_account" {
  description = "Service account email used by Cloud Run"
  value       = google_service_account.cloud_run.email
}
