output "service_account_emails" {
  description = "Map of service name → service account email"
  value       = { for k, sa in google_service_account.service : k => sa.email }
}

output "cicd_service_account_email" {
  description = "CI/CD deployer service account email (bind to Workload Identity Pool)"
  value       = google_service_account.cicd.email
}

output "vpc_id" {
  description = "VPC network self-link"
  value       = google_compute_network.vpc.self_link
}

output "private_subnet_id" {
  description = "Private subnet self-link"
  value       = google_compute_subnetwork.private.self_link
}

output "artifact_registry_url" {
  description = "Base URL for Docker image pushes"
  value       = "${var.region}-docker.pkg.dev/${var.project_id}/${google_artifact_registry_repository.app.repository_id}"
}

output "audit_log_bucket" {
  description = "GCS bucket receiving all Cloud Audit Logs"
  value       = google_storage_bucket.audit_logs.name
}
