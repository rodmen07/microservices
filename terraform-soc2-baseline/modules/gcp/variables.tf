variable "project_id" {
  description = "GCP project ID"
  type        = string
}

variable "region" {
  description = "GCP region for regional resources"
  type        = string
  default     = "us-central1"
}

variable "services" {
  description = "List of service names. A dedicated service account is created per service."
  type        = list(string)
  default     = []
}

variable "log_bucket_name" {
  description = "GCS bucket name for the audit log sink. Must be globally unique."
  type        = string
}

variable "log_retention_days" {
  description = "Days to retain audit logs in the GCS bucket."
  type        = number
  default     = 365
}

variable "network_name" {
  description = "Name for the VPC network"
  type        = string
  default     = "soc2-vpc"
}

variable "subnet_cidr" {
  description = "CIDR range for the primary private subnet"
  type        = string
  default     = "10.10.0.0/24"
}

# ---------------------------------------------------------------------------
# CC9.2 — Vendor Risk variables
# ---------------------------------------------------------------------------

variable "vendor_secrets" {
  description = <<-EOT
    List of Secret Manager secrets that belong to third-party vendors.
    Each entry is labeled with the vendor name and tier for SOC 2 CC9.2 inventory.
    Example: [{ secret_id = "STRIPE_API_KEY", vendor = "Stripe", tier = "critical" }]
  EOT
  type = list(object({
    secret_id = string
    vendor    = string
    tier      = string  # critical | standard | low
  }))
  default = []
}

variable "vendor_services" {
  description = <<-EOT
    List of Cloud Run services that integrate with external vendors.
    Each service will be labeled with the SOC 2 CC9.2 review attestation.
    Example: [{ service_name = "observaboard" }]
  EOT
  type = list(object({
    service_name = string
  }))
  default = []
}

variable "vendor_review_date" {
  description = "ISO-8601 date of the most recent vendor risk review (YYYY-MM-DD). Used as a label value on secrets and services."
  type        = string
  default     = "2026-05-07"
}

variable "reviewer_email_hash" {
  description = "SHA-256 prefix (first 8 chars) of the reviewer's email address. Stored as a label to identify the attestor without exposing PII."
  type        = string
  default     = "unknown"
}

variable "error_rate_threshold" {
  description = "Number of 5xx responses per second (averaged over 60s windows) that triggers the CC9.2 availability alert."
  type        = number
  default     = 1
}

variable "notification_channel_ids" {
  description = "List of Cloud Monitoring notification channel resource names to alert on CC9.2 availability degradation."
  type        = list(string)
  default     = []
}
