# ---------------------------------------------------------------------------
# CC6.7 — Secrets Management (GCP Secret Manager)
#
# Secrets are stored in Secret Manager with auto-replication.
# Access is bound to specific service accounts — no allUsers or allAuthenticatedUsers.
# Secret values are NOT stored in Terraform state; they must be set out-of-band
# using `gcloud secrets versions add` or the console after `terraform apply`.
# ---------------------------------------------------------------------------

# Example: JWT secret shared across services
resource "google_secret_manager_secret" "jwt_secret" {
  project   = var.project_id
  secret_id = "AUTH_JWT_SECRET"

  replication {
    auto {}
  }

  labels = {
    managed-by = "terraform-soc2-baseline"
    control    = "cc6-7"
  }

  lifecycle {
    prevent_destroy = true
  }
}

# Example: Per-service database URL secrets
resource "google_secret_manager_secret" "database_url" {
  for_each = toset(var.services)

  project   = var.project_id
  secret_id = "DATABASE_URL_${upper(replace(each.key, "-", "_"))}"

  replication {
    auto {}
  }

  labels = {
    service    = each.key
    managed-by = "terraform-soc2-baseline"
    control    = "cc6-7"
  }

  lifecycle {
    prevent_destroy = true
  }
}

# Bind each service SA to its own database URL secret only
resource "google_secret_manager_secret_iam_member" "service_db_secret" {
  for_each = toset(var.services)

  project   = var.project_id
  secret_id = google_secret_manager_secret.database_url[each.key].secret_id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.service[each.key].email}"
}

# All services share the JWT secret (read-only)
resource "google_secret_manager_secret_iam_member" "service_jwt_secret" {
  for_each = toset(var.services)

  project   = var.project_id
  secret_id = google_secret_manager_secret.jwt_secret.secret_id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.service[each.key].email}"
}

# ---------------------------------------------------------------------------
# NOTE: To add a secret value after apply:
#   echo -n "your-secret-value" | \
#     gcloud secrets versions add AUTH_JWT_SECRET --data-file=-
# ---------------------------------------------------------------------------
