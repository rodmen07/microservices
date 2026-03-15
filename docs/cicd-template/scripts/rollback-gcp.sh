#!/usr/bin/env bash
# rollback-gcp.sh — Revert GCP Cloud Run service to the previous revision
#
# Finds the most recently deployed revision before the current one and
# shifts 100% of traffic to it. The failed revision receives 0% traffic.
#
# Usage:
#   SERVICE=my-service REGION=us-central1 ./rollback-gcp.sh
#
# Environment variables:
#   SERVICE   Cloud Run service name (required)
#   REGION    GCP region (required)

set -euo pipefail

SERVICE="${SERVICE:?SERVICE must be set}"
REGION="${REGION:?REGION must be set}"

echo "Rollback: finding previous revision for ${SERVICE} in ${REGION}"

# List revisions sorted by creation time (newest first), skip the first (current)
PREVIOUS=$(gcloud run revisions list \
  --service="$SERVICE" \
  --region="$REGION" \
  --format="value(metadata.name)" \
  --sort-by="~metadata.creationTimestamp" \
  --limit=2 | tail -1)

if [ -z "$PREVIOUS" ]; then
  echo "Rollback FAILED: no previous revision found for ${SERVICE}"
  exit 1
fi

echo "Rolling back to revision: ${PREVIOUS}"

gcloud run services update-traffic "$SERVICE" \
  --region="$REGION" \
  --to-revisions="${PREVIOUS}=100"

echo "Rollback complete: 100% traffic now on ${PREVIOUS}"
