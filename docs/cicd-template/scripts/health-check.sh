#!/usr/bin/env bash
# health-check.sh — Cloud-agnostic service health poller
#
# Polls SERVICE_URL/health every INTERVAL seconds until it returns HTTP 200
# or MAX_WAIT seconds have elapsed.
#
# Usage:
#   SERVICE_URL=https://my-service.example.com ./health-check.sh
#
# Environment variables:
#   SERVICE_URL   Base URL of the service (required, no trailing slash)
#   MAX_WAIT      Seconds to wait before declaring failure (default: 90)
#   INTERVAL      Seconds between polls (default: 10)

set -euo pipefail

SERVICE_URL="${SERVICE_URL:?SERVICE_URL must be set}"
MAX_WAIT="${MAX_WAIT:-90}"
INTERVAL="${INTERVAL:-10}"
elapsed=0

echo "Health check: polling ${SERVICE_URL}/health (timeout: ${MAX_WAIT}s, interval: ${INTERVAL}s)"

while [ "$elapsed" -lt "$MAX_WAIT" ]; do
  STATUS=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "${SERVICE_URL}/health" || echo "000")

  if [ "$STATUS" = "200" ]; then
    echo "Health check passed (${elapsed}s elapsed, HTTP ${STATUS})"
    exit 0
  fi

  echo "  HTTP ${STATUS} — retrying in ${INTERVAL}s (${elapsed}s elapsed)"
  sleep "$INTERVAL"
  elapsed=$((elapsed + INTERVAL))
done

echo "Health check FAILED: service did not return 200 within ${MAX_WAIT}s"
exit 1
