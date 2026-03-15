# ---------------------------------------------------------------------------
# CC6.1 / CC6.2 / CC6.3 — IAM Least-Privilege
#
# Each service gets a dedicated IAM role with scoped inline policies.
# No wildcard actions (*) on production resources.
# OIDC is used for CI/CD — no long-lived access keys.
# ---------------------------------------------------------------------------

data "aws_caller_identity" "current" {}
data "aws_region" "current" {}

# ---------------------------------------------------------------------------
# Per-service ECS task execution roles
# ---------------------------------------------------------------------------

data "aws_iam_policy_document" "ecs_assume" {
  statement {
    effect  = "Allow"
    actions = ["sts:AssumeRole"]
    principals {
      type        = "Service"
      identifiers = ["ecs-tasks.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "service_task" {
  for_each = toset(var.services)

  name               = "${var.environment}-${each.key}-task-role"
  assume_role_policy = data.aws_iam_policy_document.ecs_assume.json
  description        = "Least-privilege task role for ${each.key}. Managed by terraform-soc2-baseline."

  tags = {
    Service     = each.key
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
  }
}

# Allow each service to read its own secrets — scoped to its prefix
resource "aws_iam_role_policy" "service_secrets" {
  for_each = toset(var.services)

  name = "read-own-secrets"
  role = aws_iam_role.service_task[each.key].id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "ReadOwnSecrets"
        Effect = "Allow"
        Action = [
          "secretsmanager:GetSecretValue",
          "secretsmanager:DescribeSecret",
        ]
        # Scoped to this service's secret prefix only — no wildcard resource
        Resource = "arn:aws:secretsmanager:${data.aws_region.current.name}:${data.aws_caller_identity.current.account_id}:secret:${var.environment}/${each.key}/*"
      },
      {
        Sid      = "DecryptWithCMK"
        Effect   = "Allow"
        Action   = ["kms:Decrypt", "kms:GenerateDataKey"]
        Resource = aws_kms_key.secrets.arn
      }
    ]
  })
}

# ECS task execution role (shared) — pulls images + logs
resource "aws_iam_role" "ecs_execution" {
  name               = "${var.environment}-ecs-execution-role"
  assume_role_policy = data.aws_iam_policy_document.ecs_assume.json
  description        = "ECS task execution role. ECR pull + CloudWatch Logs write."

  tags = {
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
  }
}

resource "aws_iam_role_policy_attachment" "ecs_execution_managed" {
  role       = aws_iam_role.ecs_execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

# ---------------------------------------------------------------------------
# CC6.2 — CI/CD OIDC Role (no long-lived access keys)
# ---------------------------------------------------------------------------

data "aws_iam_policy_document" "github_oidc_assume" {
  statement {
    effect  = "Allow"
    actions = ["sts:AssumeRoleWithWebIdentity"]
    principals {
      type        = "Federated"
      identifiers = ["arn:aws:iam::${data.aws_caller_identity.current.account_id}:oidc-provider/token.actions.githubusercontent.com"]
    }
    condition {
      test     = "StringLike"
      variable = "token.actions.githubusercontent.com:sub"
      # Restrict to your org/repo — update before use
      values = ["repo:YOUR_GITHUB_ORG/YOUR_REPO:*"]
    }
    condition {
      test     = "StringEquals"
      variable = "token.actions.githubusercontent.com:aud"
      values   = ["sts.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "cicd_deployer" {
  name               = "${var.environment}-cicd-deployer"
  assume_role_policy = data.aws_iam_policy_document.github_oidc_assume.json
  description        = "Assumed by GitHub Actions via OIDC. No long-lived keys."

  tags = {
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
  }
}

resource "aws_iam_role_policy" "cicd_deploy" {
  name = "deploy-permissions"
  role = aws_iam_role.cicd_deployer.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "ECRPush"
        Effect = "Allow"
        Action = [
          "ecr:GetAuthorizationToken",
          "ecr:BatchCheckLayerAvailability",
          "ecr:InitiateLayerUpload",
          "ecr:UploadLayerPart",
          "ecr:CompleteLayerUpload",
          "ecr:PutImage",
        ]
        Resource = "*" # ECR auth token requires * resource
      },
      {
        Sid    = "ECSDeployScoped"
        Effect = "Allow"
        Action = [
          "ecs:UpdateService",
          "ecs:DescribeServices",
          "ecs:RegisterTaskDefinition",
          "ecs:ListTaskDefinitions",
        ]
        Resource = "arn:aws:ecs:${data.aws_region.current.name}:${data.aws_caller_identity.current.account_id}:*"
      },
      {
        Sid      = "PassExecutionRole"
        Effect   = "Allow"
        Action   = "iam:PassRole"
        Resource = aws_iam_role.ecs_execution.arn
      }
    ]
  })
}
