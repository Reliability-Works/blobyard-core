#!/usr/bin/env bash

set -euo pipefail
umask 077

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
RUN_ID="${BASHPID:-$$}"
BLOBYARD_CORE_PORT="${BLOBYARD_CORE_PORT:-$((20000 + RUN_ID % 20000))}"
export BLOBYARD_CORE_PORT

ACTIVE_PROJECTS=()

cleanup() {
  local project
  local compose_file
  for entry in "${ACTIVE_PROJECTS[@]:-}"; do
    project=${entry%%:*}
    compose_file=${entry#*:}
    docker compose --project-name "$project" --file "$compose_file" \
      down --volumes --remove-orphans >/dev/null 2>&1 || true
  done
}
trap cleanup EXIT HUP INT TERM

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'Required operator acceptance tool is unavailable: %s\n' "$1" >&2
    exit 1
  }
}

assert_health() {
  local response
  response=$(curl --fail --silent --show-error --connect-timeout 2 --max-time 30 \
    --retry 30 --retry-delay 1 --retry-connrefused \
    "http://127.0.0.1:$BLOBYARD_CORE_PORT/v1/health")
  grep -Eq '"status":[[:space:]]*"ok"' <<<"$response" || {
    printf 'Blob Yard health response did not report an ok status.\n' >&2
    exit 1
  }
}

assert_reconciliation() {
  local project="$1"
  local compose_file="$2"
  shift 2
  local report
  report=$(docker compose --project-name "$project" --file "$compose_file" exec -T core \
    /usr/local/bin/blobyard-server reconcile --data-dir /var/lib/blobyard/data "$@")
  grep -Eq '"clean":[[:space:]]*true' <<<"$report" || {
    printf 'Blob Yard reconciliation did not report a clean store.\n' >&2
    exit 1
  }
  grep -Eq '"findings":[[:space:]]*0' <<<"$report" || {
    printf 'Blob Yard reconciliation reported findings.\n' >&2
    exit 1
  }
}

run_stack() {
  local name="$1"
  local compose_file="$2"
  local restart_dependency="$3"
  shift 3
  local project="blobyard-compose-$name-$RUN_ID"

  ACTIVE_PROJECTS+=("$project:$compose_file")
  docker compose --project-name "$project" --file "$compose_file" \
    up --build --detach --wait --wait-timeout 180
  assert_health
  docker compose --project-name "$project" --file "$compose_file" exec -T core \
    /usr/local/bin/blobyard-server healthcheck --url http://127.0.0.1:8787/v1/health
  assert_reconciliation "$project" "$compose_file" "$@"

  if [[ -n "$restart_dependency" ]]; then
    docker compose --project-name "$project" --file "$compose_file" \
      restart core "$restart_dependency"
  else
    docker compose --project-name "$project" --file "$compose_file" restart core
  fi
  docker compose --project-name "$project" --file "$compose_file" \
    up --detach --wait --wait-timeout 180
  assert_health
  assert_reconciliation "$project" "$compose_file" "$@"

  docker compose --project-name "$project" --file "$compose_file" \
    down --volumes --remove-orphans
  ACTIVE_PROJECTS=()
  printf 'Blob Yard %s Compose acceptance passed.\n' "$name"
}

require_command curl
require_command docker
require_command grep
docker info >/dev/null
docker compose version >/dev/null

run_stack filesystem "$REPO_ROOT/deploy/compose/filesystem.yaml" ""
run_stack minio "$REPO_ROOT/deploy/compose/minio.yaml" minio \
  --storage s3 \
  --s3-endpoint http://minio:9000 \
  --s3-bucket blobyard \
  --s3-force-path-style

printf 'Blob Yard operator Compose acceptance passed.\n'
