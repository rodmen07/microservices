#!/usr/bin/env bash
set -euo pipefail

# Detect root based on script location so it works from /d and /mnt/d paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CONTINUE_MODE=0

for arg in "$@"; do
  case "$arg" in
    --continue)
      CONTINUE_MODE=1
      ;;
    --root=*)
      ROOT="${arg#--root=}"
      ;;
    *)
      # Backward-compatible positional root argument.
      ROOT="$arg"
      ;;
  esac
done

services=(
  "accounts-service"
  "activities-service"
  "automation-service"
  "contacts-service"
  "integrations-service"
  "opportunities-service"
  "reporting-service"
  "search-service"
  "standalones/backend-service"
)

postgres_integration_services=(
  "accounts-service"
  "activities-service"
  "contacts-service"
)

requires_test_database() {
  local svc="$1"
  for pg_svc in "${postgres_integration_services[@]}"; do
    if [[ "$svc" == "$pg_svc" ]]; then
      return 0
    fi
  done
  return 1
}

echo "==> Rust checks root: $ROOT"

failures=()

for service in "${services[@]}"; do
  service_path="$ROOT/$service"

  if [[ ! -f "$service_path/Cargo.toml" ]]; then
    echo "==> Skipping $service (no Cargo.toml found)"
    continue
  fi

  echo "==> $service"
  if (
    cd "$service_path"
    cargo fmt --all
    cargo clippy --all-targets --all-features -- -D warnings
    if requires_test_database "$service" && [[ -z "${TEST_DATABASE_URL:-}" ]]; then
      echo "==> TEST_DATABASE_URL is unset; running library tests only for $service"
      cargo test --lib
    else
      cargo test
    fi
  ); then
    echo "==> PASS $service"
  else
    echo "==> FAIL $service"
    failures+=("$service")
    if [[ $CONTINUE_MODE -eq 0 ]]; then
      echo "==> Stopping on first failure (use --continue to keep going)"
      exit 1
    fi
  fi
done

echo "==> WSL Rust checks completed"

if [[ ${#failures[@]} -gt 0 ]]; then
  echo "==> Failure summary"
  for service in "${failures[@]}"; do
    echo " - $service"
  done
  exit 1
fi
