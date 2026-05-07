# ---------------------------------------------------------------------------
# CC9.2 — Vendor and Third-Party Risk Management
#
# SOC 2 CC9.2 requires the organization to assess, monitor, and manage risks
# associated with vendors and third-party service providers. This module
# implements controls in three areas:
#
#   1. Service dependency inventory — a Secret Manager label convention that
#      tags every secret with the upstream vendor it belongs to, enabling a
#      machine-readable software-bill-of-materials for third-party services.
#
#   2. Vendor risk labels — every Cloud Run service is tagged with
#      `soc2_vendor_reviewed = true` and `soc2_last_reviewed = <date>` so
#      the compliance dashboard can surface services whose review has lapsed.
#
#   3. Monitoring alert — a Cloud Monitoring alert fires when any Cloud Run
#      service starts returning 5xx errors at a rate above the threshold,
#      giving the on-call team early warning of a vendor-side degradation.
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# 1. Vendor inventory — Secret Manager labels
#
# Usage: pass var.vendor_secrets to tag secrets with their owning vendor.
# Example:
#   vendor_secrets = [
#     { secret_id = "STRIPE_API_KEY",  vendor = "Stripe",  tier = "critical" },
#     { secret_id = "SENDGRID_API_KEY", vendor = "SendGrid", tier = "standard" },
#   ]
# ---------------------------------------------------------------------------

resource "google_secret_manager_secret" "vendor_secret" {
  for_each = {
    for s in var.vendor_secrets : s.secret_id => s
  }

  project   = var.project_id
  secret_id = each.value.secret_id

  replication {
    auto {}
  }

  labels = {
    soc2_control         = "cc9-2"
    soc2_vendor          = lower(replace(each.value.vendor, " ", "-"))
    soc2_vendor_tier     = each.value.tier
    soc2_last_reviewed   = var.vendor_review_date
    managed_by           = "terraform-soc2-baseline"
  }

  lifecycle {
    # The secret payload is managed outside Terraform (e.g. via gcloud secrets versions add).
    # Prevent accidental deletion of secrets that may hold live credentials.
    prevent_destroy = true
    ignore_changes  = [labels["soc2_last_reviewed"]]
  }
}

# ---------------------------------------------------------------------------
# 2. Cloud Run service labels — vendor review attestation
#
# Attaches SOC 2 CC9.2 labels to each Cloud Run service that integrates with
# an external vendor. Auditors can query these labels to confirm reviews are
# current (i.e. soc2_last_reviewed within the past 12 months).
# ---------------------------------------------------------------------------

resource "google_cloud_run_v2_service" "vendor_tagged" {
  for_each = {
    for s in var.vendor_services : s.service_name => s
  }

  project  = var.project_id
  name     = each.value.service_name
  location = var.region

  # We only manage labels here; the full service spec is owned by the
  # deploy-cloud-run.yml CI pipeline. Using lifecycle.ignore_changes prevents
  # Terraform from overwriting the image or env vars set by CI.
  template {
    labels = {
      soc2_control           = "cc9-2"
      soc2_vendor_reviewed   = "true"
      soc2_last_reviewed     = var.vendor_review_date
      soc2_reviewer          = var.reviewer_email_hash
    }

    containers {
      # Placeholder image — replaced by CI on every deploy.
      # lifecycle.ignore_changes below ensures Terraform does not revert it.
      image = "gcr.io/cloudrun/placeholder"
    }
  }

  labels = {
    soc2_control         = "cc9-2"
    soc2_vendor_reviewed = "true"
    soc2_last_reviewed   = var.vendor_review_date
  }

  lifecycle {
    ignore_changes = [
      template[0].containers,
      template[0].service_account,
      template[0].volumes,
      template[0].vpc_access,
    ]
  }
}

# ---------------------------------------------------------------------------
# 3. Availability monitoring — Cloud Run 5xx alert
#
# Fires when any monitored Cloud Run service exceeds the configured 5xx error
# rate over a 5-minute window. The notification channel (PagerDuty / email)
# is supplied via var.notification_channel_ids.
# ---------------------------------------------------------------------------

resource "google_monitoring_alert_policy" "vendor_service_5xx" {
  project      = var.project_id
  display_name = "CC9.2 - Vendor-integrated service availability degradation"
  combiner     = "OR"

  documentation {
    content = <<-EOT
      ## SOC 2 CC9.2 - Vendor Service Availability Alert

      One or more Cloud Run services that integrate with a third-party vendor
      are returning HTTP 5xx responses above the acceptable threshold.

      **Immediate actions:**
      1. Check the Cloud Run logs for the affected service.
      2. Verify the upstream vendor status page.
      3. If vendor-side, open a support ticket and document in the vendor risk register.
      4. If self-inflicted, roll back the latest deployment.

      **SLO reference:** 99.5% availability target per CC9.2 vendor risk acceptance criteria.
    EOT
    mime_type = "text/markdown"
  }

  conditions {
    display_name = "Cloud Run request error rate > threshold"

    condition_threshold {
      filter          = <<-EOT
        resource.type = "cloud_run_revision"
        AND metric.type = "run.googleapis.com/request_count"
        AND metric.labels.response_code_class = "5xx"
      EOT
      duration        = "300s"
      comparison      = "COMPARISON_GT"
      threshold_value = var.error_rate_threshold

      aggregations {
        alignment_period     = "60s"
        per_series_aligner   = "ALIGN_RATE"
        cross_series_reducer = "REDUCE_SUM"
        group_by_fields      = ["resource.labels.service_name"]
      }
    }
  }

  notification_channels = var.notification_channel_ids

  alert_strategy {
    auto_close = "604800s"  # 7 days

    notification_rate_limit {
      period = "3600s"  # at most one notification per hour per policy
    }
  }

  user_labels = {
    soc2_control = "cc9-2"
    managed_by   = "terraform-soc2-baseline"
  }
}
