locals {
  # PostgreSQL connection strings per service via Cloud SQL Auth Proxy Unix socket.
  # Cloud Run connects through the connector (--add-cloudsql-instances), not direct IP.
  #
  # sqlx 0.8 cannot parse the ?host= query parameter format — it raises
  # Configuration(EmptyHost) when the URL authority is empty (user:pass@/db).
  # The correct format is to percent-encode the socket directory path as the
  # URL host: postgresql://user:pass@%2Fcloudsql%2FPROJECT%3AREGION%3AINSTANCE/db
  # sqlx decodes the host to /cloudsql/... and treats it as a Unix socket path.
  instance_connection_name = google_sql_database_instance.main.connection_name

  # Percent-encode the socket path for use as a URL host component.
  # /cloudsql/PROJECT:REGION:INSTANCE → %2Fcloudsql%2FPROJECT%3AREGION%3AINSTANCE
  encoded_socket = replace(replace("/cloudsql/${google_sql_database_instance.main.connection_name}", "/", "%2F"), ":", "%3A")

  database_urls = {
    accounts      = "postgresql://accounts_user:${var.db_password}@${local.encoded_socket}/accounts"
    contacts      = "postgresql://contacts_user:${var.db_password}@${local.encoded_socket}/contacts"
    tasks         = "postgresql://tasks_user:${var.db_password}@${local.encoded_socket}/tasks"
    activities    = "postgresql://activities_user:${var.db_password}@${local.encoded_socket}/activities"
    automation    = "postgresql://automation_user:${var.db_password}@${local.encoded_socket}/automation"
    integrations  = "postgresql://integrations_user:${var.db_password}@${local.encoded_socket}/integrations"
    opportunities = "postgresql://opportunities_user:${var.db_password}@${local.encoded_socket}/opportunities"
    reporting     = "postgresql://reporting_user:${var.db_password}@${local.encoded_socket}/reporting"
    search        = "postgresql://search_user:${var.db_password}@${local.encoded_socket}/search"
    spend         = "postgresql://spend_user:${var.db_password}@${local.encoded_socket}/spend"
    projects      = "postgresql://projects_user:${var.db_password}@${local.encoded_socket}/projects"
    audit         = "postgresql://audit_user:${var.db_password}@${local.encoded_socket}/audit"
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
