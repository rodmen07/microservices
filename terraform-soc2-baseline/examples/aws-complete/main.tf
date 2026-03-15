module "soc2_aws" {
  source = "../../modules/aws"

  region               = "us-east-1"
  environment          = "production"
  services             = ["accounts", "contacts", "backend"]
  vpc_cidr             = "10.20.0.0/16"
  private_subnet_cidrs = ["10.20.1.0/24", "10.20.2.0/24"]
  public_subnet_cidrs  = ["10.20.101.0/24", "10.20.102.0/24"]
  trail_bucket_name    = "your-org-cloudtrail-prod"
  log_retention_days   = 365
}

output "ecr_urls" {
  value = module.soc2_aws.ecr_repository_urls
}

output "cicd_role" {
  value = module.soc2_aws.cicd_role_arn
}

output "vpc_id" {
  value = module.soc2_aws.vpc_id
}
