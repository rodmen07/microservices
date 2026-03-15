#!/usr/bin/env bash
# rollback-aws.sh — Revert AWS ECS service to the previous task definition
#
# Finds the task definition ARN of the last stable deployment and updates
# the service to use it. Works with Fargate and EC2 launch types.
#
# Usage:
#   CLUSTER=my-cluster SERVICE=my-service REGION=us-east-1 ./rollback-aws.sh
#
# Environment variables:
#   CLUSTER   ECS cluster name (required)
#   SERVICE   ECS service name (required)
#   REGION    AWS region (required)

set -euo pipefail

CLUSTER="${CLUSTER:?CLUSTER must be set}"
SERVICE="${SERVICE:?SERVICE must be set}"
REGION="${REGION:?REGION must be set}"

echo "Rollback: finding previous task definition for ${SERVICE} in cluster ${CLUSTER}"

# Get the task definition of the last completed (stable) deployment
# Deployments are ordered newest-first; index [1] is the one before the failed one
PREVIOUS_TD=$(aws ecs describe-services \
  --cluster "$CLUSTER" \
  --services "$SERVICE" \
  --region "$REGION" \
  --query 'services[0].deployments | sort_by(@, &createdAt) | [-2].taskDefinition' \
  --output text)

if [ -z "$PREVIOUS_TD" ] || [ "$PREVIOUS_TD" = "None" ]; then
  echo "Rollback FAILED: no previous task definition found for ${SERVICE}"
  exit 1
fi

echo "Rolling back to task definition: ${PREVIOUS_TD}"

aws ecs update-service \
  --cluster "$CLUSTER" \
  --service "$SERVICE" \
  --task-definition "$PREVIOUS_TD" \
  --region "$REGION" \
  --query 'service.{service:serviceName,td:taskDefinition,status:status}' \
  --output table

echo "Rollback initiated. Monitor with:"
echo "  aws ecs describe-services --cluster ${CLUSTER} --services ${SERVICE} --region ${REGION}"
