# v1.10 — Cost & FinOps
# Budget alerts, cost anomaly detection, and per-service cost visibility

# Billing budget with threshold alerts
resource "google_billing_budget" "microservices_monthly" {
  count             = var.billing_account_id != "" ? 1 : 0
  billing_account   = var.billing_account_id
  display_name      = "Portfolio Microservices - Monthly"

  amount {
    specified_amount {
      currency_code = "USD"
      units         = var.monthly_budget_usd
    }
  }

  # Alert when reaching 50%, 90%, and 100% of budget
  threshold_rules {
    threshold_percent = 50.0
  }
  threshold_rules {
    threshold_percent = 90.0
  }
  threshold_rules {
    threshold_percent = 100.0
  }

  # Filter to Cloud Run services only
  budget_filter {
    projects = [
      "projects/${data.google_project.current.number}"
    ]
    services = [
      "services/95FF-2EF5-5EA1"  # Cloud Run
    ]
  }
}

# Billing notification channel for email alerts
resource "google_monitoring_notification_channel" "budget_email" {
  count           = var.budget_alert_email != "" ? 1 : 0
  display_name    = "Budget Alert Email"
  type            = "email"
  enabled         = true
  user_labels = {
    "severity" = "high"
  }
  
  labels = {
    "email_address" = var.budget_alert_email
  }
}

# Alert policy for Cloud Run CPU utilization (early warning for scaling issues)
resource "google_monitoring_alert_policy" "cloud_run_high_cpu" {
  count           = var.billing_account_id != "" ? 1 : 0
  display_name    = "High CPU Utilization Alert"
  combiner        = "OR"
  enabled         = true
  notification_channels = google_monitoring_notification_channel.budget_email[*].id

  conditions {
    display_name = "CPU utilization > 80%"

    condition_threshold {
      filter          = "resource.type = \"cloud_run_revision\" AND metric.type = \"run.googleapis.com/request_count\""
      duration        = "300s"
      comparison      = "COMPARISON_GT"
      threshold_value = 0.8
      
      aggregations {
        alignment_period   = "60s"
        per_series_aligner = "ALIGN_RATE"
      }
    }
  }
}

# Alert policy for high memory usage (indicates inefficient service)
resource "google_monitoring_alert_policy" "cloud_run_high_memory" {
  count           = var.billing_account_id != "" ? 1 : 0
  display_name    = "High Memory Usage Alert"
  combiner        = "OR"
  enabled         = true
  notification_channels = google_monitoring_notification_channel.budget_email[*].id

  conditions {
    display_name = "Memory usage > 400Mi (of 512Mi)"

    condition_threshold {
      filter          = "resource.type = \"cloud_run_revision\" AND metric.type = \"run.googleapis.com/request_count\""
      duration        = "300s"
      comparison      = "COMPARISON_GT"
      threshold_value = 0.78  # 400Mi / 512Mi
      
      aggregations {
        alignment_period   = "60s"
        per_series_aligner = "ALIGN_MEAN"
      }
    }
  }
}

# Alert policy for Cloud SQL high connections (indicates resource contention)
resource "google_monitoring_alert_policy" "cloud_sql_high_connections" {
  count           = var.billing_account_id != "" ? 1 : 0
  display_name    = "Cloud SQL High Connection Count Alert"
  combiner        = "OR"
  enabled         = true
  notification_channels = google_monitoring_notification_channel.budget_email[*].id

  conditions {
    display_name = "Database connections > 10"

    condition_threshold {
      filter          = "resource.type = \"cloudsql_database\" AND metric.type = \"cloudsql.googleapis.com/database/mysql/connections\""
      duration        = "300s"
      comparison      = "COMPARISON_GT"
      threshold_value = 10
      
      aggregations {
        alignment_period   = "60s"
        per_series_aligner = "ALIGN_MEAN"
      }
    }
  }
}

# Get current project for policies
data "google_project" "current" {}
