# Single Docker repository for all microservice images
resource "google_artifact_registry_repository" "microservices" {
  location      = var.region
  repository_id = "microservices"
  description   = "Docker images for all microservices"
  format        = "DOCKER"

  depends_on = [google_project_service.apis]
}

# Grant the Cloud Run SA permission to pull images
resource "google_artifact_registry_repository_iam_member" "cloud_run_reader" {
  location   = google_artifact_registry_repository.microservices.location
  repository = google_artifact_registry_repository.microservices.name
  role       = "roles/artifactregistry.reader"
  member     = "serviceAccount:${google_service_account.cloud_run.email}"
}
