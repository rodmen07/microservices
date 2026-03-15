module "soc2_gcp" {
  source = "../../modules/gcp"

  project_id         = "your-gcp-project-id"
  region             = "us-central1"
  services           = ["accounts", "contacts", "backend"]
  log_bucket_name    = "your-org-audit-logs-prod"
  log_retention_days = 365
  network_name       = "prod-vpc"
  subnet_cidr        = "10.10.0.0/24"
}

output "service_accounts" {
  value = module.soc2_gcp.service_account_emails
}

output "cicd_sa" {
  value = module.soc2_gcp.cicd_service_account_email
}

output "registry" {
  value = module.soc2_gcp.artifact_registry_url
}
