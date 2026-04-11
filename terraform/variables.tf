variable "project_id" {
  description = "GCP project ID"
  type        = string
}

variable "region" {
  description = "GCP region for all resources"
  type        = string
  default     = "us-south1"
}

variable "db_instance_name" {
  description = "Cloud SQL instance name (must be globally unique)"
  type        = string
  default     = "microservices-pg"
}

variable "db_tier" {
  description = "Cloud SQL machine tier"
  type        = string
  default     = "db-f1-micro"
}

variable "db_password" {
  description = "Master password for Cloud SQL (used per-service users)"
  type        = string
  sensitive   = true
}

variable "jwt_secret" {
  description = "Shared JWT secret for all Rust services"
  type        = string
  sensitive   = true
}

variable "anthropic_api_key" {
  description = "Anthropic API key for ai-orchestrator-service"
  type        = string
  sensitive   = true
}

variable "frontend_origin" {
  description = "GitHub Pages URL for ALLOWED_ORIGINS (e.g. https://rodmen07.github.io)"
  type        = string
  default     = "https://rodmen07.github.io"
}

variable "accounts_service_url" {
  description = "Cloud Run URL for accounts-service (used by contacts/activities for cross-service validation)"
  type        = string
  default     = ""
}

variable "contacts_service_url" {
  description = "Cloud Run URL for contacts-service (used by activities for cross-service validation)"
  type        = string
  default     = ""
}

variable "opportunities_service_url" {
  description = "Cloud Run URL for opportunities-service (used by reporting-service dashboard aggregation)"
  type        = string
  default     = ""
}

variable "activities_service_url" {
  description = "Cloud Run URL for activities-service (used by reporting-service dashboard aggregation)"
  type        = string
  default     = ""
}

variable "audit_service_url" {
  description = "Cloud Run URL for audit-service (used by CRM services to emit audit events)"
  type        = string
  default     = ""
}
