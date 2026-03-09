locals {
  # PostgreSQL connection strings per service
  # Format: postgresql://<user>:<password>@<host>/<db>?sslmode=require
  db_host = google_sql_database_instance.main.public_ip_address

  database_urls = {
    accounts     = "postgresql://accounts_user:${var.db_password}@${local.db_host}/accounts?sslmode=require"
    contacts     = "postgresql://contacts_user:${var.db_password}@${local.db_host}/contacts?sslmode=require"
    tasks        = "postgresql://tasks_user:${var.db_password}@${local.db_host}/tasks?sslmode=require"
    activities   = "postgresql://activities_user:${var.db_password}@${local.db_host}/activities?sslmode=require"
    automation   = "postgresql://automation_user:${var.db_password}@${local.db_host}/automation?sslmode=require"
    integrations = "postgresql://integrations_user:${var.db_password}@${local.db_host}/integrations?sslmode=require"
    opportunities = "postgresql://opportunities_user:${var.db_password}@${local.db_host}/opportunities?sslmode=require"
    reporting    = "postgresql://reporting_user:${var.db_password}@${local.db_host}/reporting?sslmode=require"
    search       = "postgresql://search_user:${var.db_password}@${local.db_host}/search?sslmode=require"
  }
}

# Shared JWT secret
resource "google_secret_manager_secret" "jwt_secret" {
  secret_id = "AUTH_JWT_SECRET"

  replication {
    auto {}
  }

  depends_on = [google_project_service.apis]
}

resource "google_secret_manager_secret_version" "jwt_secret" {
  secret      = google_secret_manager_secret.jwt_secret.id
  secret_data = var.jwt_secret
}

# Anthropic API key (ai-orchestrator only)
resource "google_secret_manager_secret" "anthropic_api_key" {
  secret_id = "ANTHROPIC_API_KEY"

  replication {
    auto {}
  }

  depends_on = [google_project_service.apis]
}

resource "google_secret_manager_secret_version" "anthropic_api_key" {
  secret      = google_secret_manager_secret.anthropic_api_key.id
  secret_data = var.anthropic_api_key
}

# Per-service DATABASE_URL secrets
resource "google_secret_manager_secret" "database_urls" {
  for_each  = local.database_urls
  secret_id = "DATABASE_URL_${upper(each.key)}"

  replication {
    auto {}
  }

  depends_on = [google_project_service.apis]
}

resource "google_secret_manager_secret_version" "database_urls" {
  for_each    = local.database_urls
  secret      = google_secret_manager_secret.database_urls[each.key].id
  secret_data = each.value
}
