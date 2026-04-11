# Single PostgreSQL instance shared across all services
resource "google_sql_database_instance" "main" {
  name             = var.db_instance_name
  database_version = "POSTGRES_16"
  region           = var.region

  deletion_protection = true

  settings {
    tier              = var.db_tier
    availability_type = "ZONAL"
    disk_size         = 10
    disk_type         = "PD_SSD"
    disk_autoresize   = true

    backup_configuration {
      enabled    = true
      start_time = "03:00"
    }

    ip_configuration {
      ipv4_enabled = true
      # No authorized_networks — Cloud Run connects via Cloud SQL connector
      # using INSTANCE_CONNECTION_NAME, not a public authorized IP range.
    }

    database_flags {
      name  = "max_connections"
      value = "100"
    }
  }

  depends_on = [google_project_service.apis]
}

# One database per service
locals {
  service_dbs = [
    "accounts",
    "contacts",
    "tasks",
    "activities",
    "automation",
    "integrations",
    "opportunities",
    "reporting",
    "search",
    "spend",
    "projects",
    "audit",
  ]
}

resource "google_sql_database" "service_dbs" {
  for_each = toset(local.service_dbs)

  name     = each.key
  instance = google_sql_database_instance.main.name
}

# One DB user per service (least privilege)
resource "google_sql_user" "service_users" {
  for_each = toset(local.service_dbs)

  name     = "${each.key}_user"
  instance = google_sql_database_instance.main.name
  password = var.db_password
}
