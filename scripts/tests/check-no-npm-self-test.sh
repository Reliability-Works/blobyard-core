#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
CHECKER="$SCRIPT_DIR/../check-no-npm.sh"
FIXTURES="$SCRIPT_DIR/fixtures/no-npm"
FAILURES=0

expect_rejected() {
  local tool="$1"
  local fixture="$2"
  local output_file
  output_file=$(mktemp)

  if "$CHECKER" --scan-file "$fixture" >"$output_file" 2>&1; then
    printf 'FAIL: expected rejection for %s\n' "$fixture" >&2
    FAILURES=$((FAILURES + 1))
  elif ! grep -F "[$tool]" "$output_file" >/dev/null 2>&1; then
    printf 'FAIL: rejection did not identify %s for %s\n' "$tool" "$fixture" >&2
    FAILURES=$((FAILURES + 1))
  else
    printf 'PASS: rejected %s fixture\n' "$tool"
  fi

  rm -f "$output_file"
}

expect_accepted() {
  local fixture="$1"

  if "$CHECKER" --scan-file "$fixture" >/dev/null; then
    printf 'PASS: accepted %s\n' "$(basename "$fixture")"
  else
    printf 'FAIL: expected acceptance for %s\n' "$fixture" >&2
    FAILURES=$((FAILURES + 1))
  fi
}

expect_rejected "npm" "$FIXTURES/forbidden-shell.sh"
expect_rejected "npx" "$FIXTURES/forbidden-doc.md"
expect_rejected "yarn" "$FIXTURES/forbidden-workflow.yml"
expect_rejected "npm" "$FIXTURES/forbidden-package.json"
expect_rejected "bun" "$FIXTURES/forbidden-bun.sh"
expect_accepted "$FIXTURES/allowed-policy.md"
expect_accepted "$FIXTURES/allowed-pnpm.sh"
expect_accepted "$FIXTURES/allowed-package.json"

if [[ "$FAILURES" -ne 0 ]]; then
  printf 'No-package-manager policy self-test failed: %s case(s).\n' "$FAILURES" >&2
  exit 1
fi

printf 'No-package-manager policy self-test passed.\n'
