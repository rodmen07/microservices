output "vpc_id" {
  description = "VPC ID"
  value       = aws_vpc.main.id
}

output "private_subnet_ids" {
  description = "Private subnet IDs (use for ECS tasks)"
  value       = aws_subnet.private[*].id
}

output "public_subnet_ids" {
  description = "Public subnet IDs (use for load balancers only)"
  value       = aws_subnet.public[*].id
}

output "app_security_group_id" {
  description = "Security group ID for application containers"
  value       = aws_security_group.app.id
}

output "alb_security_group_id" {
  description = "Security group ID for the load balancer"
  value       = aws_security_group.alb.id
}

output "ecr_repository_urls" {
  description = "Map of service name → ECR repository URL"
  value       = { for k, repo in aws_ecr_repository.app : k => repo.repository_url }
}

output "kms_key_arn" {
  description = "KMS CMK ARN used for secrets and log encryption"
  value       = aws_kms_key.secrets.arn
}

output "cicd_role_arn" {
  description = "IAM role ARN for GitHub Actions OIDC assumption"
  value       = aws_iam_role.cicd_deployer.arn
}

output "ecs_cluster_name" {
  description = "ECS cluster name"
  value       = aws_ecs_cluster.main.name
}

output "cloudtrail_bucket" {
  description = "S3 bucket receiving CloudTrail logs"
  value       = aws_s3_bucket.trail.id
}

output "service_task_role_arns" {
  description = "Map of service name → ECS task role ARN"
  value       = { for k, role in aws_iam_role.service_task : k => role.arn }
}
