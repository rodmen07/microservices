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

# Workspace service crate -> its logical database, matching the CI job list
# in .github/workflows/rust.yml exactly (same crates, same database names).
# A local PostgreSQL at localhost:5432 provides them (docker-compose up db
# creates the databases via scripts/postgres-init/).
services=(
  "accounts-service:accounts"
  "contacts-service:contacts"
  "activities-service:activities"
  "automation-service:workflows"
  "integrations-service:connections"
  "opportunities-service:opportunities"
  "reporting-service:reports"
  "search-service:documents"
  "spend-service:spend"
  "projects-service:projects"
  "audit-service:audit"
)

echo "==> Rust checks root: $ROOT"

failures=()

for entry in "${services[@]}"; do
  service="${entry%%:*}"
  database="${entry##*:}"
  service_path="$ROOT/$service"

  if [[ ! -f "$service_path/Cargo.toml" ]]; then
    echo "==> Skipping $service (no Cargo.toml found)"
    continue
  fi

  echo "==> $service"
  if (
    cd "$service_path"
    export DATABASE_URL="postgres://postgres:postgres@localhost:5432/$database"
    cargo fmt --all
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test
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
