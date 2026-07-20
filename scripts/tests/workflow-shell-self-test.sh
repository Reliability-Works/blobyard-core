#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/../.." && pwd)
FIXTURE_DIR=$(mktemp -d "${TMPDIR:-/tmp}/blobyard-workflow-shell-test.XXXXXX")
trap 'rm -rf "$FIXTURE_DIR"' EXIT

write_workflow() {
  local path="$1"
  local run_line="$2"
  printf '%s\n' \
    'jobs:' \
    '  validate:' \
    '    runs-on: ubuntu-latest' \
    '    steps:' \
    '      - name: Probe' \
    "        run: $run_line" >"$path"
}

write_workflow "$FIXTURE_DIR/valid.yml" "printf '%s\\n' \"\$HOME\""
node "$REPO_ROOT/scripts/check-workflow-shell.mjs" "$FIXTURE_DIR/valid.yml"

write_workflow "$FIXTURE_DIR/invalid.yml" "printf '%s\\n' \$HOME"
if node "$REPO_ROOT/scripts/check-workflow-shell.mjs" "$FIXTURE_DIR/invalid.yml" >/dev/null 2>&1; then
  printf 'workflow shell self-test accepted an unsafe unquoted expansion\n' >&2
  exit 1
fi

printf 'workflow shell self-test passed\n'
