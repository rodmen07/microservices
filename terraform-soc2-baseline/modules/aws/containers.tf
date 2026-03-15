# ---------------------------------------------------------------------------
# CC6.8 / A1.2 — Non-Root Containers + Availability (AWS)
#
# ECR repository with image scanning on push.
# ECS task definition template enforcing non-root user (UID 65534 = nobody).
# Multi-AZ ECS service for A1.2 availability.
# ---------------------------------------------------------------------------

resource "aws_ecr_repository" "app" {
  for_each             = toset(var.services)
  name                 = "${var.environment}/${each.key}"
  image_tag_mutability = "IMMUTABLE" # prevents tag overwrite — audit trail

  image_scanning_configuration {
    scan_on_push = true # CC6.8 — detect vulnerabilities before deployment
  }

  encryption_configuration {
    encryption_type = "KMS"
    kms_key         = aws_kms_key.secrets.arn
  }

  tags = {
    Service     = each.key
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
    Control     = "CC6.8"
  }
}

# ECR lifecycle policy: keep last 10 production images, expire untagged after 1 day
resource "aws_ecr_lifecycle_policy" "app" {
  for_each   = toset(var.services)
  repository = aws_ecr_repository.app[each.key].name

  policy = jsonencode({
    rules = [
      {
        rulePriority = 1
        description  = "Expire untagged images after 1 day"
        selection = {
          tagStatus   = "untagged"
          countType   = "sinceImagePushed"
          countUnit   = "days"
          countNumber = 1
        }
        action = { type = "expire" }
      },
      {
        rulePriority = 2
        description  = "Keep last 10 tagged images"
        selection = {
          tagStatus     = "tagged"
          tagPrefixList = ["prod-", "staging-"]
          countType     = "imageCountMoreThan"
          countNumber   = 10
        }
        action = { type = "expire" }
      }
    ]
  })
}

# ---------------------------------------------------------------------------
# CC6.8 — Non-Root ECS Task Definition Template
#
# This is a reference task definition showing the required non-root settings.
# Your application task definitions MUST set:
#   user = "65534"             (nobody — UID available in distroless/slim images)
#   readonlyRootFilesystem = true
#   privileged = false
#   allowPrivilegeEscalation = false  (via linuxParameters)
# ---------------------------------------------------------------------------

# ECS Cluster
resource "aws_ecs_cluster" "main" {
  name = "soc2-${var.environment}"

  setting {
    name  = "containerInsights"
    value = "enabled" # CC7.2 — enables Container Insights metrics
  }

  tags = {
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
    Control     = "A1.2"
  }
}

# Reference task definition for the first service (demonstrates non-root pattern)
resource "aws_ecs_task_definition" "reference" {
  count                    = length(var.services) > 0 ? 1 : 0
  family                   = "soc2-${var.environment}-${var.services[0]}"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = "256"
  memory                   = "512"
  execution_role_arn       = aws_iam_role.ecs_execution.arn
  task_role_arn            = aws_iam_role.service_task[var.services[0]].arn

  container_definitions = jsonencode([
    {
      name  = var.services[0]
      image = "${aws_ecr_repository.app[var.services[0]].repository_url}:latest"

      # CC6.8 — Non-root user enforcement
      user = "65534" # nobody

      # Reduce attack surface
      readonlyRootFilesystem = true
      privileged             = false

      linuxParameters = {
        capabilities = {
          drop = ["ALL"] # drop all Linux capabilities
        }
        initProcessEnabled = false
      }

      portMappings = [{ containerPort = 8080, protocol = "tcp" }]

      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = aws_cloudwatch_log_group.app.name
          "awslogs-region"        = var.region
          "awslogs-stream-prefix" = var.services[0]
        }
      }

      environment = []   # inject via Secrets Manager references, not plaintext
      secrets = [
        {
          name      = "DATABASE_URL"
          valueFrom = aws_secretsmanager_secret.database_url[var.services[0]].arn
        }
      ]
    }
  ])

  tags = {
    Service     = var.services[0]
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
    Control     = "CC6.8"
  }
}

# ECS Service — multi-AZ placement for A1.2 availability
resource "aws_ecs_service" "reference" {
  count           = length(var.services) > 0 ? 1 : 0
  name            = "soc2-${var.environment}-${var.services[0]}"
  cluster         = aws_ecs_cluster.main.id
  task_definition = aws_ecs_task_definition.reference[0].arn
  desired_count   = 2 # A1.2 — minimum 2 for HA
  launch_type     = "FARGATE"

  network_configuration {
    subnets          = aws_subnet.private[*].id
    security_groups  = [aws_security_group.app.id]
    assign_public_ip = false # private subnet only
  }

  deployment_minimum_healthy_percent = 100
  deployment_maximum_percent         = 200

  tags = {
    Service     = var.services[0]
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
    Control     = "A1.2"
  }
}
