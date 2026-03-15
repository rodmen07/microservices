# ---------------------------------------------------------------------------
# CC7.2 / CC7.3 / CC8.1 — Audit Logging (AWS)
#
# CloudTrail: multi-region trail with S3 log file validation + versioning.
# CloudWatch Logs: 365-day retention for container and application logs.
# Includes a skeleton CloudWatch alarm for root account activity.
# ---------------------------------------------------------------------------

# S3 bucket for CloudTrail logs
resource "aws_s3_bucket" "trail" {
  bucket        = var.trail_bucket_name
  force_destroy = false

  tags = {
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
    Control     = "CC7.2"
  }
}

resource "aws_s3_bucket_versioning" "trail" {
  bucket = aws_s3_bucket.trail.id
  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "trail" {
  bucket = aws_s3_bucket.trail.id
  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm     = "aws:kms"
      kms_master_key_id = aws_kms_key.secrets.arn
    }
  }
}

# Block all public access to the trail bucket
resource "aws_s3_bucket_public_access_block" "trail" {
  bucket                  = aws_s3_bucket.trail.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

# Bucket policy: allow CloudTrail to write logs
resource "aws_s3_bucket_policy" "trail" {
  bucket = aws_s3_bucket.trail.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "AWSCloudTrailAclCheck"
        Effect = "Allow"
        Principal = { Service = "cloudtrail.amazonaws.com" }
        Action    = "s3:GetBucketAcl"
        Resource  = aws_s3_bucket.trail.arn
      },
      {
        Sid    = "AWSCloudTrailWrite"
        Effect = "Allow"
        Principal = { Service = "cloudtrail.amazonaws.com" }
        Action    = "s3:PutObject"
        Resource  = "${aws_s3_bucket.trail.arn}/AWSLogs/${data.aws_caller_identity.current.account_id}/*"
        Condition = {
          StringEquals = { "s3:x-amz-acl" = "bucket-owner-full-control" }
        }
      }
    ]
  })
}

# CloudTrail: multi-region, log file validation enabled
resource "aws_cloudtrail" "main" {
  name                          = "soc2-${var.environment}-trail"
  s3_bucket_name                = aws_s3_bucket.trail.id
  include_global_service_events = true
  is_multi_region_trail         = true
  enable_log_file_validation    = true
  kms_key_id                    = aws_kms_key.secrets.arn

  event_selector {
    read_write_type           = "All"
    include_management_events = true

    # Log all S3 object-level events (for the trail bucket itself)
    data_resource {
      type   = "AWS::S3::Object"
      values = ["${aws_s3_bucket.trail.arn}/"]
    }

    # Log Secrets Manager secret value access
    data_resource {
      type   = "AWS::SecretsManager::Secret"
      values = ["arn:aws:secretsmanager:*"]
    }
  }

  tags = {
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
    Control     = "CC7.2"
  }

  depends_on = [aws_s3_bucket_policy.trail]
}

# CloudWatch log group for application logs
resource "aws_cloudwatch_log_group" "app" {
  name              = "/soc2/${var.environment}/app"
  retention_in_days = var.log_retention_days
  kms_key_id        = aws_kms_key.secrets.arn

  tags = {
    Environment = var.environment
    ManagedBy   = "terraform-soc2-baseline"
    Control     = "CC7.2"
  }
}

# ---------------------------------------------------------------------------
# CC7.3 — Incident Detection: alarm on root account usage
# ---------------------------------------------------------------------------

resource "aws_cloudwatch_metric_alarm" "root_account_usage" {
  alarm_name          = "soc2-${var.environment}-root-account-usage"
  alarm_description   = "SOC2 CC7.3 — root account was used. Investigate immediately."
  metric_name         = "RootAccountUsage"
  namespace           = "CloudTrailMetrics"
  statistic           = "Sum"
  period              = 300
  evaluation_periods  = 1
  threshold           = 1
  comparison_operator = "GreaterThanOrEqualToThreshold"
  treat_missing_data  = "notBreaching"

  # Add alarm_actions = [aws_sns_topic.alerts.arn] after creating an SNS topic
}

# CloudWatch metric filter to feed the alarm
resource "aws_cloudwatch_log_metric_filter" "root_account_usage" {
  name           = "soc2-root-account-usage"
  log_group_name = aws_cloudwatch_log_group.app.name
  pattern        = "{ $.userIdentity.type = \"Root\" && $.userIdentity.invokedBy NOT EXISTS && $.eventType != \"AwsServiceEvent\" }"

  metric_transformation {
    name      = "RootAccountUsage"
    namespace = "CloudTrailMetrics"
    value     = "1"
  }
}
