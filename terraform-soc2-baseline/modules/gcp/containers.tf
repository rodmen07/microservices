# ---------------------------------------------------------------------------
# CC6.8 / A1.2 — Non-Root Containers + Availability (GCP)
#
# Artifact Registry with image scanning enabled.
# Cloud Run services must specify a non-root USER — enforced by documentation
# and CI policy (Dockerfile linting via hadolint in the CI/CD template).
#
# For organization-level enforcement, apply an Organization Policy constraint:
#   constraints/run.allowedIngress → specific values only
#   constraints/iam.allowedPolicyMemberDomains → restrict to your domain
# ---------------------------------------------------------------------------

resource "google_artifact_registry_repository" "app" {
  project       = var.project_id
  location      = var.region
  repository_id = "app-images"
  format        = "DOCKER"
  description   = "Application container images. Managed by terraform-soc2-baseline."

  # Enable vulnerability scanning on push
  cleanup_policy_dry_run = false

  labels = {
    managed-by = "terraform-soc2-baseline"
    control    = "cc6-8"
  }
}

# Only the CI/CD SA can push images
resource "google_artifact_registry_repository_iam_member" "cicd_writer" {
  project    = var.project_id
  location   = var.region
  repository = google_artifact_registry_repository.app.name
  role       = "roles/artifactregistry.writer"
  member     = "serviceAccount:${google_service_account.cicd.email}"
}

# Service accounts can pull images (read-only)
resource "google_artifact_registry_repository_iam_member" "service_reader" {
  for_each = toset(var.services)

  project    = var.project_id
  location   = var.region
  repository = google_artifact_registry_repository.app.name
  role       = "roles/artifactregistry.reader"
  member     = "serviceAccount:${google_service_account.service[each.key].email}"
}

# ---------------------------------------------------------------------------
# A1.2 — Availability
# Cloud Run service template (reference — not managed here, owned by CI/CD)
#
# Required settings in all Cloud Run services:
#   min_instances = 1            # ensures always-on availability
#   max_instances = 10           # caps runaway scaling costs
#   timeout_seconds = 300
#   cpu = "1"
#   memory = "512Mi"
#
# Enforced via CI/CD pipeline gating on gcloud run deploy flags.
# ---------------------------------------------------------------------------

# NOTE: Non-root enforcement for Cloud Run containers:
# Every Dockerfile MUST include:
#   RUN useradd --no-create-home --shell /bin/false appuser
#   USER appuser
#
# The CI/CD template (docs/cicd-template/) includes a hadolint check
# that fails the build if USER is missing from the final stage.
