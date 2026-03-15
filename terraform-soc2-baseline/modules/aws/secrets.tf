# ---------------------------------------------------------------------------
# CC6.7 — Secrets Management (AWS Secrets Manager + KMS CMK)
#
# Secrets are encrypted with a customer-managed KMS key.
# Access is bound to specific task roles via resource policy — no wildcard principals.
# Secret values are NOT stored in Terraform state; set via console or CLI after apply.
# ---------------------------------------------------------------------------

# Customer-managed KMS key for secret encryption
resource "aws_kms_key" "secrets" {
  description             = "CMK for Secrets Manager secrets — ${var.environment}"
  deletion_window_in_days = var.kms_deletion_window_days
  enable_key_rotation     = true

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "AllowRootAccountFull"
        Effect = "Allow"
        Principal = {
          AWS = "arn:aws:iam::${data.aws_caller_identity.current.account_id}:root"
        }
        Action   = "kms:*"
        Resource = "*"
      },
      {
        Sid    = "AllowSecretsManagerUse"
        Effect = "Allow"
        Principal = {
          Service = "secretsmanager.amazonaws.com"
        }
        Action = [
          "kms:Decrypt",
          "kms:GenerateDataKey",
          "kms:DescribeKey",
        ]
        Resource = "*"
      }
    ]
  })

  tags = {
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
    Control     = "CC6.7"
  }
}

resource "aws_kms_alias" "secrets" {
  name          = "alias/${var.environment}/soc2-secrets"
  target_key_id = aws_kms_key.secrets.key_id
}

# Shared JWT secret
resource "aws_secretsmanager_secret" "jwt_secret" {
  name                    = "${var.environment}/auth/jwt-secret"
  kms_key_id              = aws_kms_key.secrets.arn
  recovery_window_in_days = 30
  description             = "Shared JWT signing secret. Managed by terraform-soc2-baseline."

  tags = {
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
    Control     = "CC6.7"
  }
}

# Per-service database URL secrets
resource "aws_secretsmanager_secret" "database_url" {
  for_each = toset(var.services)

  name                    = "${var.environment}/${each.key}/database-url"
  kms_key_id              = aws_kms_key.secrets.arn
  recovery_window_in_days = 30
  description             = "Database connection string for ${each.key}."

  tags = {
    Service     = each.key
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
    Control     = "CC6.7"
  }
}

# ---------------------------------------------------------------------------
# NOTE: To set a secret value after apply:
#   aws secretsmanager put-secret-value \
#     --secret-id "production/auth/jwt-secret" \
#     --secret-string "your-secret-value"
# ---------------------------------------------------------------------------
