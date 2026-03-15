variable "region" {
  description = "AWS region for regional resources"
  type        = string
  default     = "us-east-1"
}

variable "environment" {
  description = "Environment label (e.g. staging, production)"
  type        = string
  default     = "production"
}

variable "services" {
  description = "List of service names. A dedicated IAM role is created per service."
  type        = list(string)
  default     = []
}

variable "vpc_cidr" {
  description = "CIDR block for the VPC"
  type        = string
  default     = "10.20.0.0/16"
}

variable "private_subnet_cidrs" {
  description = "CIDR blocks for private subnets (one per AZ)"
  type        = list(string)
  default     = ["10.20.1.0/24", "10.20.2.0/24"]
}

variable "public_subnet_cidrs" {
  description = "CIDR blocks for public subnets (one per AZ, used by NAT gateways)"
  type        = list(string)
  default     = ["10.20.101.0/24", "10.20.102.0/24"]
}

variable "trail_bucket_name" {
  description = "S3 bucket name for CloudTrail logs. Must be globally unique."
  type        = string
}

variable "log_retention_days" {
  description = "CloudWatch log group retention in days"
  type        = number
  default     = 365
}

variable "kms_deletion_window_days" {
  description = "Waiting period (days) before KMS key deletion. Min 7, max 30."
  type        = number
  default     = 30
}
