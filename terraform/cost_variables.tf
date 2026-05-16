# v1.10 — Cost & FinOps Variables
# Budget alerts, per-service cost tracking, and cloud scaling optimization

variable "billing_account_id" {
  description = "GCP billing account ID for budget alerts (format: 012345-ABCDEF-GHIJKL). Leave empty to skip budget alerts."
  type        = string
  default     = ""
}

variable "monthly_budget_usd" {
  description = "Monthly budget threshold in USD for cost alerts. Alerts trigger at 50%, 90%, and 100%. (0 = disable)"
  type        = number
  default     = 500
}

variable "budget_alert_email" {
  description = "Email address to receive GCP billing budget alert notifications (e.g. team@example.com)"
  type        = string
  default     = ""
}

variable "service_min_instances" {
  description = "Per-service minimum Cloud Run instances. 0 = scale-to-zero (batch workloads), 1+ = always warm (reduces cold starts). Use locals in Terraform to override defaults."
  type        = map(number)
  default = {
    # Critical path services: always warm (1-2 warm instances prevent cold starts)
    "accounts-service"      = 1
    "audit-service"         = 1
    "projects-service"      = 1
    "backend-service"       = 1

    # High-traffic services: semi-warm
    "contacts-service"      = 1
    "activities-service"    = 1
    "opportunities-service" = 1

    # Batch/asynchronous workloads: scale-to-zero (cost optimization)
    "automation-service"    = 0
    "integrations-service"  = 0
    "reporting-service"     = 0
    "search-service"        = 0
    "spend-service"         = 0
  }
}
