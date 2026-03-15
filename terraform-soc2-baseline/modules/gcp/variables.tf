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
