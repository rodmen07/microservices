# ---------------------------------------------------------------------------
# CC7.2 / CC7.3 / CC8.1 — Audit Logging (GCP)
#
# Enables Data Access audit logs for Secret Manager, Cloud SQL, and Cloud Run.
# Routes all audit logs to a GCS bucket for long-term retention.
# Includes a skeleton Cloud Monitoring alert for unusual secret access.
# ---------------------------------------------------------------------------

# GCS bucket for audit log retention
resource "google_storage_bucket" "audit_logs" {
  project       = var.project_id
  name          = var.log_bucket_name
  location      = "US"
  force_destroy = false

  uniform_bucket_level_access = true

  versioning {
    enabled = true
  }

  lifecycle_rule {
    action { type = "Delete" }
    condition { age = var.log_retention_days }
  }

  labels = {
    managed-by = "terraform-soc2-baseline"
    control    = "cc7-2"
  }
}

# Deny public access to audit logs
resource "google_storage_bucket_iam_binding" "audit_logs_no_public" {
  bucket = google_storage_bucket.audit_logs.name
  role   = "roles/storage.objectViewer"
  members = [
    # Only the log sink SA gets read access — no allUsers, no allAuthenticatedUsers
  ]
}

# Log sink: route all project audit logs to GCS
resource "google_logging_project_sink" "audit_sink" {
  project                = var.project_id
  name                   = "soc2-audit-sink"
  destination            = "storage.googleapis.com/${google_storage_bucket.audit_logs.name}"
  filter                 = "logName:(cloudaudit.googleapis.com)"
  unique_writer_identity = true
  description            = "Routes Cloud Audit Logs to GCS for SOC 2 evidence. Managed by terraform-soc2-baseline."
}

# Grant the sink's writer identity access to write to the bucket
resource "google_storage_bucket_iam_member" "audit_sink_writer" {
  bucket = google_storage_bucket.audit_logs.name
  role   = "roles/storage.objectCreator"
  member = google_logging_project_sink.audit_sink.writer_identity
}

# ---------------------------------------------------------------------------
# CC7.2 — Data Access Audit Logging
# Enable DATA_READ and DATA_WRITE audit logs for sensitive services
# ---------------------------------------------------------------------------

resource "google_project_iam_audit_config" "secret_manager" {
  project = var.project_id
  service = "secretmanager.googleapis.com"

  audit_log_config {
    log_type = "DATA_READ"
  }
  audit_log_config {
    log_type = "DATA_WRITE"
  }
}

resource "google_project_iam_audit_config" "cloud_run" {
  project = var.project_id
  service = "run.googleapis.com"

  audit_log_config {
    log_type = "DATA_READ"
  }
  audit_log_config {
    log_type = "DATA_WRITE"
  }
}

resource "google_project_iam_audit_config" "cloud_sql" {
  project = var.project_id
  service = "sqladmin.googleapis.com"

  audit_log_config {
    log_type = "DATA_READ"
  }
  audit_log_config {
    log_type = "DATA_WRITE"
  }
}

# ---------------------------------------------------------------------------
# CC7.3 — Incident Detection: alert on unusual secret access volume
# (skeleton — set notification_channels after apply)
# ---------------------------------------------------------------------------

resource "google_monitoring_alert_policy" "secret_access_spike" {
  project      = var.project_id
  display_name = "SOC2 — Unusual Secret Manager Access"
  combiner     = "OR"

  conditions {
    display_name = "Secret Manager DATA_READ spike"
    condition_threshold {
      filter          = "resource.type=\"audited_resource\" AND protoPayload.serviceName=\"secretmanager.googleapis.com\" AND protoPayload.methodName:\"AccessSecretVersion\""
      duration        = "300s"
      comparison      = "COMPARISON_GT"
      threshold_value = 100

      aggregations {
        alignment_period   = "300s"
        per_series_aligner = "ALIGN_RATE"
      }
    }
  }

  alert_strategy {
    auto_close = "86400s"
  }

  # Add notification_channels = ["projects/.../notificationChannels/..."] after apply
}
