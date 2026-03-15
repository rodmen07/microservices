# ---------------------------------------------------------------------------
# CC6.1 / CC6.2 / CC6.3 — IAM Least-Privilege
#
# Each service gets a dedicated service account with only the roles it needs.
# No roles/owner or roles/editor are granted anywhere.
# OIDC (Workload Identity Federation) is used for CI/CD — no SA key files.
# ---------------------------------------------------------------------------

# One service account per application service
resource "google_service_account" "service" {
  for_each = toset(var.services)

  project      = var.project_id
  account_id   = "${each.key}-sa"
  display_name = "Service account for ${each.key}"
  description  = "Least-privilege SA for ${each.key}. Managed by terraform-soc2-baseline."
}

# Allow each SA to read secrets (not write, not administer)
resource "google_project_iam_member" "secret_accessor" {
  for_each = toset(var.services)

  project = var.project_id
  role    = "roles/secretmanager.secretAccessor"
  member  = "serviceAccount:${google_service_account.service[each.key].email}"
}

# Cloud SQL client access — connection only, no schema admin
resource "google_project_iam_member" "cloudsql_client" {
  for_each = toset(var.services)

  project = var.project_id
  role    = "roles/cloudsql.client"
  member  = "serviceAccount:${google_service_account.service[each.key].email}"
}

# Cloud Run invoker — allow services to call each other
resource "google_project_iam_member" "run_invoker" {
  for_each = toset(var.services)

  project = var.project_id
  role    = "roles/run.invoker"
  member  = "serviceAccount:${google_service_account.service[each.key].email}"
}

# ---------------------------------------------------------------------------
# CC6.2 — CI/CD Workload Identity Federation (no long-lived SA keys)
#
# Grants the GitHub Actions OIDC token the ability to impersonate each SA.
# Set var.services to the services your CI pipeline deploys.
# ---------------------------------------------------------------------------

resource "google_service_account" "cicd" {
  project      = var.project_id
  account_id   = "cicd-deployer"
  display_name = "CI/CD Deployer"
  description  = "Used by GitHub Actions via WIF — no key files. Managed by terraform-soc2-baseline."
}

# CI/CD deployer needs Cloud Run developer + Artifact Registry writer
resource "google_project_iam_member" "cicd_run_developer" {
  project = var.project_id
  role    = "roles/run.developer"
  member  = "serviceAccount:${google_service_account.cicd.email}"
}

resource "google_project_iam_member" "cicd_registry_writer" {
  project = var.project_id
  role    = "roles/artifactregistry.writer"
  member  = "serviceAccount:${google_service_account.cicd.email}"
}

# NOTE: To complete WIF setup, create a Workload Identity Pool and Provider
# in the GCP console or via google_iam_workload_identity_pool resources,
# then bind the pool to this service account.
# See: https://cloud.google.com/iam/docs/workload-identity-federation-with-deployment-pipelines
