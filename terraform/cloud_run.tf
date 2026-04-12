locals {
  registry_base = "${var.region}-docker.pkg.dev/${var.project_id}/microservices"

  # Map service name → { db_key, port, extra_env }
  # extra_env: plain-value env vars injected in addition to DATABASE_URL, AUTH_JWT_SECRET, ALLOWED_ORIGINS.
  rust_services = {
    "accounts-service"     = { db_key = "accounts",      port = 8080, extra_env = {
      AUDIT_SERVICE_URL = var.audit_service_url
    }}
    "contacts-service"     = { db_key = "contacts",      port = 8080, extra_env = {
      ACCOUNTS_SERVICE_URL = var.accounts_service_url
      AUDIT_SERVICE_URL    = var.audit_service_url
    }}
    "activities-service"   = { db_key = "activities",    port = 8080, extra_env = {
      ACCOUNTS_SERVICE_URL = var.accounts_service_url
      CONTACTS_SERVICE_URL = var.contacts_service_url
      AUDIT_SERVICE_URL    = var.audit_service_url
    }}
    "automation-service"   = { db_key = "automation",    port = 8080, extra_env = {} }
    "integrations-service" = { db_key = "integrations",  port = 8080, extra_env = {} }
    "opportunities-service" = { db_key = "opportunities", port = 8080, extra_env = {
      AUDIT_SERVICE_URL = var.audit_service_url
    }}
    "reporting-service"    = { db_key = "reporting",     port = 8080, extra_env = {
      ACCOUNTS_SERVICE_URL  = var.accounts_service_url
      CONTACTS_SERVICE_URL  = var.contacts_service_url
      OPPORTUNITIES_SERVICE_URL = var.opportunities_service_url
      ACTIVITIES_SERVICE_URL    = var.activities_service_url
    }}
    "search-service"       = { db_key = "search",        port = 8080, extra_env = {} }
    "spend-service"        = { db_key = "spend",         port = 8080, extra_env = {} }
    "audit-service"        = { db_key = "audit",         port = 8080, extra_env = {
      OBSERVABOARD_INGEST_URL = var.observaboard_ingest_url
      OBSERVABOARD_API_KEY    = var.observaboard_api_key
    }}
  }

  # backend-service (task-api) uses "tasks" DB
  task_service = {
    db_key = "tasks"
    port   = 8080
  }
}

# 8 workspace Rust services
resource "google_cloud_run_v2_service" "rust_services" {
  for_each = local.rust_services

  name     = each.key
  location = var.region

  ingress = "INGRESS_TRAFFIC_ALL"

  template {
    service_account = google_service_account.cloud_run.email

    scaling {
      min_instance_count = 0
      max_instance_count = 3
    }

    volumes {
      name = "cloudsql"
      cloud_sql_instance {
        instances = [google_sql_database_instance.main.connection_name]
      }
    }

    containers {
      # Bootstrap with a public image; CI later deploys service-specific images.
      image = "us-docker.pkg.dev/cloudrun/container/hello:latest"

      ports {
        container_port = each.value.port
      }

      volume_mounts {
        name       = "cloudsql"
        mount_path = "/cloudsql"
      }

      env {
        name  = "ALLOWED_ORIGINS"
        value = var.frontend_origin
      }

      env {
        name = "DATABASE_URL"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.database_urls[each.value.db_key].secret_id
            version = "latest"
          }
        }
      }

      env {
        name = "AUTH_JWT_SECRET"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.jwt_secret.secret_id
            version = "latest"
          }
        }
      }

      dynamic "env" {
        for_each = each.value.extra_env
        content {
          name  = env.key
          value = env.value
        }
      }

      resources {
        limits = {
          cpu    = "1"
          memory = "512Mi"
        }
      }
    }
  }

  lifecycle {
    ignore_changes = [
      template[0].containers[0].image,
    ]
  }

  depends_on = [
    google_project_service.apis,
    google_secret_manager_secret_version.database_urls,
    google_secret_manager_secret_version.jwt_secret,
    google_artifact_registry_repository.microservices,
    google_sql_database_instance.main,
  ]
}

# backend-service (task-api, standalones)
resource "google_cloud_run_v2_service" "task_api" {
  name     = "backend-service"
  location = var.region

  ingress = "INGRESS_TRAFFIC_ALL"

  template {
    service_account = google_service_account.cloud_run.email

    scaling {
      min_instance_count = 0
      max_instance_count = 3
    }

    volumes {
      name = "cloudsql"
      cloud_sql_instance {
        instances = [google_sql_database_instance.main.connection_name]
      }
    }

    containers {
      image = "us-docker.pkg.dev/cloudrun/container/hello:latest"

      ports {
        container_port = 8080
      }

      volume_mounts {
        name       = "cloudsql"
        mount_path = "/cloudsql"
      }

      env {
        name  = "ALLOWED_ORIGINS"
        value = var.frontend_origin
      }

      env {
        name = "DATABASE_URL"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.database_urls["tasks"].secret_id
            version = "latest"
          }
        }
      }

      env {
        name = "AUTH_JWT_SECRET"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.jwt_secret.secret_id
            version = "latest"
          }
        }
      }

      resources {
        limits = {
          cpu    = "1"
          memory = "512Mi"
        }
      }
    }
  }

  lifecycle {
    ignore_changes = [
      template[0].containers[0].image,
    ]
  }

  depends_on = [
    google_project_service.apis,
    google_secret_manager_secret_version.database_urls,
    google_secret_manager_secret_version.jwt_secret,
    google_artifact_registry_repository.microservices,
    google_sql_database_instance.main,
  ]
}

# ai-orchestrator-service (Python, no DB)
# Internal-only: only reachable from other Cloud Run services in this project.
resource "google_cloud_run_v2_service" "ai_orchestrator" {
  name     = "ai-orchestrator-service"
  location = var.region

  ingress = "INGRESS_TRAFFIC_INTERNAL_LOAD_BALANCER"

  template {
    service_account = google_service_account.cloud_run.email

    scaling {
      min_instance_count = 0
      max_instance_count = 2
    }

    containers {
      image = "us-docker.pkg.dev/cloudrun/container/hello:latest"

      ports {
        container_port = 8080
      }

      env {
        name = "ANTHROPIC_API_KEY"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.anthropic_api_key.secret_id
            version = "latest"
          }
        }
      }

      env {
        name = "AUTH_JWT_SECRET"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.jwt_secret.secret_id
            version = "latest"
          }
        }
      }

      resources {
        limits = {
          cpu    = "1"
          memory = "512Mi"
        }
      }
    }
  }

  lifecycle {
    ignore_changes = [
      template[0].containers[0].image,
    ]
  }

  depends_on = [
    google_project_service.apis,
    google_secret_manager_secret_version.anthropic_api_key,
    google_secret_manager_secret_version.jwt_secret,
    google_artifact_registry_repository.microservices,
  ]
}

# Public APIs — unauthenticated invocation allowed (JWT enforced at app layer)
resource "google_cloud_run_v2_service_iam_member" "public_rust" {
  for_each = local.rust_services

  project  = var.project_id
  location = var.region
  name     = google_cloud_run_v2_service.rust_services[each.key].name
  role     = "roles/run.invoker"
  member   = "allUsers"
}

resource "google_cloud_run_v2_service_iam_member" "public_task_api" {
  project  = var.project_id
  location = var.region
  name     = google_cloud_run_v2_service.task_api.name
  role     = "roles/run.invoker"
  member   = "allUsers"
}

# AI orchestrator is internal-only — only the Cloud Run service account may invoke it
resource "google_cloud_run_v2_service_iam_member" "internal_ai_orchestrator" {
  project  = var.project_id
  location = var.region
  name     = google_cloud_run_v2_service.ai_orchestrator.name
  role     = "roles/run.invoker"
  member   = "serviceAccount:${google_service_account.cloud_run.email}"
}
