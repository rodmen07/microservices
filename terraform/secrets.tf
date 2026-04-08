locals {
  # PostgreSQL connection strings per service via Cloud SQL Auth Proxy Unix socket.
  # Cloud Run connects through the connector (--add-cloudsql-instances), not direct IP.
  # Format: postgresql://<user>:<password>@/<db>?host=/cloudsql/<instance_connection_name>
  instance_connection_name = google_sql_database_instance.main.connection_name

  database_urls = {
    accounts      = "postgresql://accounts_user:${var.db_password}@/accounts?host=/cloudsql/${local.instance_connection_name}"
    contacts      = "postgresql://contacts_user:${var.db_password}@/contacts?host=/cloudsql/${local.instance_connection_name}"
    tasks         = "postgresql://tasks_user:${var.db_password}@/tasks?host=/cloudsql/${local.instance_connection_name}"
    activities    = "postgresql://activities_user:${var.db_password}@/activities?host=/cloudsql/${local.instance_connection_name}"
    automation    = "postgresql://automation_user:${var.db_password}@/automation?host=/cloudsql/${local.instance_connection_name}"
    integrations  = "postgresql://integrations_user:${var.db_password}@/integrations?host=/cloudsql/${local.instance_connection_name}"
    opportunities = "postgresql://opportunities_user:${var.db_password}@/opportunities?host=/cloudsql/${local.instance_connection_name}"
    reporting     = "postgresql://reporting_user:${var.db_password}@/reporting?host=/cloudsql/${local.instance_connection_name}"
    search        = "postgresql://search_user:${var.db_password}@/search?host=/cloudsql/${local.instance_connection_name}"
    spend         = "postgresql://spend_user:${var.db_password}@/spend?host=/cloudsql/${local.instance_connection_name}"
    projects      = "postgresql://projects_user:${var.db_password}@/projects?host=/cloudsql/${local.instance_connection_name}"
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
