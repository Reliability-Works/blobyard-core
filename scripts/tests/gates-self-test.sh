#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/../.." && pwd)
# shellcheck source=scripts/lib/common.sh
source "$REPO_ROOT/scripts/lib/common.sh"

blobyard_prepend_mise_shims
cd "$REPO_ROOT"

RAW_TEMP_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/blobyard-core-gates.XXXXXX")
TEMP_ROOT=$(cd "$RAW_TEMP_ROOT" && pwd -P)
trap 'rm -rf "$TEMP_ROOT"' EXIT

FAILURES=0
CASE_NUMBER=0

require_tooling() {
  local command_name

  for command_name in actionlint bash gitleaks node pnpm shfmt; do
    blobyard_require_command "$command_name" "Run scripts/bootstrap.sh first."
  done
  for executable in prettier jscpd; do
    if [[ ! -x "$REPO_ROOT/node_modules/.bin/$executable" ]]; then
      blobyard_die "$executable is missing from the pinned workspace dependencies."
    fi
  done
}

expect_rejected() {
  local label="$1"
  local diagnostic="$2"
  local output_file
  shift 2

  CASE_NUMBER=$((CASE_NUMBER + 1))
  output_file="$TEMP_ROOT/case-$CASE_NUMBER.log"
  if "$@" >"$output_file" 2>&1; then
    printf 'FAIL: %s accepted its negative control.\n' "$label" >&2
    FAILURES=$((FAILURES + 1))
  elif ! grep -Eq "$diagnostic" "$output_file"; then
    printf 'FAIL: %s failed without its expected diagnostic.\n' "$label" >&2
    sed -n '1,40p' "$output_file" >&2
    FAILURES=$((FAILURES + 1))
  else
    printf 'PASS: %s rejected its negative control.\n' "$label"
  fi
}

create_prettier_fixture() {
  mkdir -p "$TEMP_ROOT/prettier"
  printf '%s\n' 'const value={answer:42}' >"$TEMP_ROOT/prettier/drift.ts"
}

reject_prettier_drift() {
  pnpm exec prettier --check "$TEMP_ROOT/prettier/drift.ts" \
    --config "$REPO_ROOT/.prettierrc.json" \
    --ignore-path "$REPO_ROOT/.prettierignore"
}

create_shell_fixtures() {
  mkdir -p "$TEMP_ROOT/shell"
  printf '%s\n' '#!/usr/bin/env bash' 'if true;then' 'printf x' 'fi' \
    >"$TEMP_ROOT/shell/drift.sh"
  printf '%s\n' '#!/usr/bin/env bash' 'if true; then' '  printf x' \
    >"$TEMP_ROOT/shell/syntax.sh"
}

reject_shell_drift() {
  shfmt -d -i 2 -ci "$TEMP_ROOT/shell/drift.sh"
}

reject_shell_syntax() {
  bash -n "$TEMP_ROOT/shell/syntax.sh"
}

create_workflow_fixture() {
  mkdir -p "$TEMP_ROOT/workflow"
  printf '%s\n' 'name: Invalid workflow' 'on: push' \
    >"$TEMP_ROOT/workflow/invalid.yml"
}

reject_invalid_workflow() {
  actionlint "$TEMP_ROOT/workflow/invalid.yml"
}

create_workflow_shell_fixture() {
  # The fixture must contain an unquoted literal $HOME.
  # shellcheck disable=SC2016
  printf '%s\n' \
    'jobs:' \
    '  validate:' \
    '    runs-on: ubuntu-latest' \
    '    steps:' \
    '      - run: printf "%s\\n" $HOME' \
    >"$TEMP_ROOT/workflow/unsafe-shell.yml"
}

reject_unsafe_workflow_shell() {
  node "$REPO_ROOT/scripts/check-workflow-shell.mjs" "$TEMP_ROOT/workflow/unsafe-shell.yml"
}

create_action_metadata_fixture() {
  local action_root="$TEMP_ROOT/action/upload"

  mkdir -p "$TEMP_ROOT/action"
  cp -R "$REPO_ROOT/.github/actions/upload" "$action_root"
  sed 's/^  project:/  removed-project:/' "$action_root/action.yml" \
    >"$action_root/action.yml.next"
  mv "$action_root/action.yml.next" "$action_root/action.yml"
}

reject_invalid_action_metadata() {
  local action_root="$TEMP_ROOT/action/upload"

  if "$action_root/tests/action-self-test.sh"; then
    return 0
  fi
  if grep -Eq '^  project:' "$action_root/action.yml"; then
    printf 'composite action contract failed before checking the mutated field\n' >&2
    return 2
  fi
  printf 'required action metadata input is missing: project\n' >&2
  return 1
}

create_duplication_fixture() {
  local name
  local duplication_root="$TEMP_ROOT/duplication"

  mkdir -p "$duplication_root"
  for name in one two; do
    printf '%s\n' \
      'export function summarizeArtifact(input: { name: string; size: number }) {' \
      '  const normalizedName = input.name.trim().toLowerCase();' \
      '  const normalizedSize = Math.max(0, input.size);' \
      '  const displayName = "artifact:" + normalizedName;' \
      '  const displaySize = "bytes:" + normalizedSize.toString();' \
      '  return { displayName, displaySize, normalizedName, normalizedSize };' \
      '}' >"$duplication_root/$name.ts"
  done
}

reject_duplication() {
  pnpm exec jscpd "$TEMP_ROOT/duplication" \
    --config "$REPO_ROOT/.jscpd.json" \
    --min-lines 5 \
    --min-tokens 50 \
    --no-tips \
    --output "$TEMP_ROOT/duplication-report" \
    --reporters console \
    --threshold 0
}

create_secret_fixture() {
  local prefix
  local secret_root="$TEMP_ROOT/secrets-repository"
  local suffix

  mkdir -p "$secret_root/scripts/lib"
  cp "$REPO_ROOT/scripts/check-secrets.sh" "$secret_root/scripts/check-secrets.sh"
  cp "$REPO_ROOT/scripts/lib/common.sh" "$secret_root/scripts/lib/common.sh"
  cp "$REPO_ROOT/.gitleaks.toml" "$secret_root/.gitleaks.toml"
  printf '%s\n' '.env.local' >"$secret_root/.gitignore"
  prefix=$(printf '%s%s' 'AK' 'IA')
  suffix=$(node -e \
    'const { createHash } = require("node:crypto"); process.stdout.write(createHash("sha256").update("blobyard-gate-self-test").digest("hex").slice(0, 16).toUpperCase());')
  printf 'access_key = "%s%s"\n' "$prefix" "$suffix" >"$secret_root/.env.local"
  git -C "$secret_root" init --quiet
  if ! "$secret_root/scripts/check-secrets.sh" working-tree >/dev/null 2>&1; then
    blobyard_die "ignored local environment files must stay outside the working-tree scan."
  fi
  git -C "$secret_root" add --force .env.local
}

reject_synthetic_secret() {
  "$TEMP_ROOT/secrets-repository/scripts/check-secrets.sh" working-tree
}

reject_staged_secret() {
  "$TEMP_ROOT/secrets-repository/scripts/check-secrets.sh" staged
}

create_policy_fixture() {
  local tool

  mkdir -p "$TEMP_ROOT/policy"
  tool=$(printf '%s%s' 'np' 'm')
  printf '%s %s\n' "$tool" install >"$TEMP_ROOT/policy/forbidden.sh"
}

reject_forbidden_package_manager() {
  "$REPO_ROOT/scripts/check-no-npm.sh" --scan-file "$TEMP_ROOT/policy/forbidden.sh"
}

prepare_fixtures() {
  create_prettier_fixture
  create_shell_fixtures
  create_workflow_fixture
  create_workflow_shell_fixture
  create_action_metadata_fixture
  create_duplication_fixture
  create_secret_fixture
  create_policy_fixture
}

run_self_tests() {
  "$REPO_ROOT/.github/actions/upload/tests/action-self-test.sh" >/dev/null

  expect_rejected 'Prettier' 'Code style issues found' reject_prettier_drift
  expect_rejected 'shfmt' '^diff ' reject_shell_drift
  expect_rejected 'shell syntax' 'syntax error' reject_shell_syntax
  expect_rejected 'actionlint workflow' 'jobs.*missing' reject_invalid_workflow
  expect_rejected 'workflow shell expansion' 'SC2086' reject_unsafe_workflow_shell
  expect_rejected 'composite action metadata' 'required action metadata input is missing' \
    reject_invalid_action_metadata
  expect_rejected 'jscpd duplication' 'Found 1 clones' reject_duplication
  expect_rejected 'gitleaks working-tree scan' 'leaks found: 1' reject_synthetic_secret
  expect_rejected 'gitleaks staged scan' 'leaks found: 1' reject_staged_secret
  expect_rejected 'package-manager policy' 'Forbidden package-manager command usage' \
    reject_forbidden_package_manager
}

require_tooling
prepare_fixtures
run_self_tests

if [[ "$FAILURES" -ne 0 ]]; then
  printf 'Gate self-test failed: %s negative control(s) did not reject correctly.\n' \
    "$FAILURES" >&2
  exit 1
fi

printf 'Gate self-test passed: %s negative controls rejected correctly.\n' "$CASE_NUMBER"
