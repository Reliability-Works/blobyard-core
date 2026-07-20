#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
# shellcheck source=scripts/lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

blobyard_prepend_mise_shims
cd "$REPO_ROOT"
blobyard_prepend_rustup_toolchain

shell_files() {
  git ls-files --cached --others --exclude-standard -z -- '*.sh' '.githooks/*'
}

staged_files() {
  git diff --cached --name-only --diff-filter=ACMR -z
}

require_js_tool() {
  local tool="$1"
  if ! pnpm exec "$tool" --version >/dev/null 2>&1; then
    blobyard_die "$tool is required; run scripts/bootstrap.sh first."
  fi
}

require_rust_lock() {
  [[ -f Cargo.lock ]] || blobyard_die "Cargo.lock is required."
}

gate_fmt() {
  local file

  blobyard_require_command pnpm "Run scripts/bootstrap.sh first."
  require_js_tool prettier
  pnpm exec prettier --check . --ignore-path .prettierignore

  while IFS= read -r -d '' file; do
    bash -n "$file"
    blobyard_require_command shfmt "Install shfmt before running formatting checks."
    shfmt -d -i 2 -ci "$file"
  done < <(shell_files)

  cargo fmt --all --check
}

gate_fmt_staged() {
  local file
  local prettier_files=()
  local rust_files=()
  local staged_shell_files=()

  while IFS= read -r -d '' file; do
    [[ -L "$file" ]] && continue
    case "$file" in
      *.rs) rust_files+=("$file") ;;
      *.sh | .githooks/*) staged_shell_files+=("$file") ;;
      *) prettier_files+=("$file") ;;
    esac
  done < <(staged_files)

  if [[ "${#prettier_files[@]}" -gt 0 ]]; then
    blobyard_require_command pnpm "Run scripts/bootstrap.sh first."
    require_js_tool prettier
    pnpm exec prettier --check --ignore-unknown --ignore-path .prettierignore \
      "${prettier_files[@]}"
  fi

  for file in "${staged_shell_files[@]}"; do
    bash -n "$file"
    blobyard_require_command shfmt "Install shfmt before running formatting checks."
    shfmt -d -i 2 -ci "$file"
  done

  if [[ "${#rust_files[@]}" -gt 0 ]]; then
    rustfmt --edition 2024 --check "${rust_files[@]}"
  fi
}

gate_no_npm() {
  "$SCRIPT_DIR/check-no-npm.sh"
  "$SCRIPT_DIR/check-no-npm.sh" --self-test
}

gate_no_npm_staged() {
  local arguments=()
  local file

  while IFS= read -r -d '' file; do
    arguments+=(--scan-file "$file")
  done < <(staged_files)

  if [[ "${#arguments[@]}" -eq 0 ]]; then
    blobyard_log_skip "no staged files require package-manager checks"
    return
  fi
  "$SCRIPT_DIR/check-no-npm.sh" "${arguments[@]}"
}

lint_shell() {
  local files=()
  local file

  blobyard_require_command shellcheck "Install shellcheck before running lint."
  while IFS= read -r -d '' file; do
    files+=("$file")
  done < <(shell_files)
  if [[ "${#files[@]}" -gt 0 ]]; then
    shellcheck -x "${files[@]}"
  fi

  node --check scripts/check-no-npm.mjs
  node --check scripts/check-workflow-shell.mjs
  node --check scripts/open-core/check-server-handler-parity.mjs
  node --check scripts/open-core/generate-conformance.mjs
  node --check scripts/openapi/contract-documents.mjs
  node --check scripts/openapi/contract-files.mjs
  node --check scripts/openapi/generate.mjs
  node --check scripts/openapi/split-contract.mjs
  node --check scripts/release/cli-version.mjs
}

lint_workflows() {
  blobyard_require_command actionlint "Install actionlint before validating GitHub workflows."
  actionlint -shellcheck= .github/workflows/*.yml
  node scripts/check-workflow-shell.mjs .github/workflows/*.yml
}

gate_lint() {
  lint_shell
  lint_workflows
  require_rust_lock
  cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
  cargo run --locked -p xtask -- check-limits
}

gate_typecheck() {
  blobyard_require_command pnpm "Run scripts/bootstrap.sh first."
  pnpm --dir sdk/typescript typecheck
  require_rust_lock
  cargo check --workspace --all-targets --all-features --locked
}

run_contract_tests() {
  local contract_test
  local tests=(
    release/tests/manifest-self-test.sh
    release/tests/surface-assets-self-test.sh
    release/tests/workflow-self-test.sh
    scripts/tests/install-self-test.sh
    scripts/tests/rust-toolchain-self-test.sh
    scripts/tests/workflow-shell-self-test.sh
    scripts/release/tests/cli-version.test.mjs
    scripts/acceptance/tests/cli-release.test.mjs
    scripts/open-core/tests/check-server-handler-parity.test.mjs
    scripts/open-core/tests/generate-conformance.test.mjs
    scripts/openapi/tests/contract-split.test.mjs
    scripts/openapi/tests/sdk.test.mjs
    .github/actions/upload/tests/action-self-test.sh
    .github/actions/upload/tests/compiled-cli-self-test.sh
  )

  for contract_test in "${tests[@]}"; do
    [[ -f "$contract_test" ]] || blobyard_die "$contract_test must exist."
    if [[ "$contract_test" == *.mjs ]]; then
      node --test "$contract_test"
    else
      [[ -x "$contract_test" ]] || blobyard_die "$contract_test must be executable."
      "$contract_test"
    fi
  done
}

gate_contracts() {
  node scripts/openapi/generate.mjs --check
  node scripts/open-core/check-server-handler-parity.mjs
  node scripts/open-core/generate-conformance.mjs --check
  run_contract_tests
}

run_rust_tests() {
  local coverage="$1"

  require_rust_lock
  if [[ "$coverage" == "coverage" ]]; then
    mkdir -p "$REPO_ROOT/target/llvm-cov"
    LLVM_PROFILE_FILE="$REPO_ROOT/target/llvm-cov/blobyard-%m-%p.profraw" \
      cargo llvm-cov nextest --workspace --all-features --locked \
      --fail-under-lines 100 \
      --fail-under-functions 100 \
      --fail-under-regions 100
  else
    cargo nextest run --workspace --all-features --locked
  fi
}

gate_test() {
  gate_contracts
  run_rust_tests coverage
}

gate_test_fast() {
  run_contract_tests
  run_rust_tests no-coverage
}

gate_rust_coverage() {
  run_rust_tests coverage
}

gate_dup() {
  "$SCRIPT_DIR/check-duplication.sh"
}

gate_secrets_staged() {
  "$SCRIPT_DIR/check-secrets.sh" staged
}

gate_secrets() {
  "$SCRIPT_DIR/check-secrets.sh" all
}

gate_audit() {
  [[ -f pnpm-lock.yaml ]] || blobyard_die "pnpm-lock.yaml is required."
  pnpm audit --prod --audit-level moderate
  require_rust_lock
  blobyard_require_command cargo-deny "Install cargo-deny."
  blobyard_require_command cargo-audit "Install cargo-audit."
  cargo deny check
  cargo audit --deny warnings
}

gate_build() {
  gate_rust_build
  pnpm --dir sdk/typescript typecheck
}

gate_rust_build() {
  require_rust_lock
  cargo build --release --locked -p blobyard-cli -p blobyard-server
}

gate_operator_acceptance() {
  "$SCRIPT_DIR/open-core/test-compose.sh"
  "$SCRIPT_DIR/open-core/test-s3-minio.sh"
}

gate_gates_self_test() {
  "$SCRIPT_DIR/tests/gates-self-test.sh"
}

gate_release_version() {
  node "$SCRIPT_DIR/release/cli-version.mjs" check
}

run_gate() {
  local name="$1"
  blobyard_log_step "$name" "running"
  "gate_${name//-/_}"
  blobyard_log_pass "$name"
}

run_all() {
  local gate
  for gate in release-version contracts fmt no-npm lint typecheck test dup secrets audit build operator-acceptance gates-self-test; do
    run_gate "$gate"
  done
}

run_static() {
  local gate
  for gate in release-version contracts fmt no-npm lint typecheck dup secrets audit gates-self-test; do
    run_gate "$gate"
  done
}

run_pre_commit() {
  local gate
  for gate in fmt-staged no-npm-staged secrets-staged; do
    run_gate "$gate"
  done
}

run_pre_push() {
  local gate
  for gate in release-version contracts fmt no-npm lint secrets; do
    run_gate "$gate"
  done
}

usage() {
  cat <<'USAGE'
Usage: scripts/check.sh <command>

Commands:
  fmt                 Check Prettier, shell formatting, and rustfmt
  no-npm              Enforce and self-test the pnpm-only policy
  lint                Run shellcheck, workflow validation, Clippy, and xtask limits
  typecheck           Run TypeScript and Rust type checks
  contracts           Verify generated contracts, conformance, installers, releases, and Action
  test                Run contract tests and exact 100 percent Rust coverage
  test-fast           Run contract and Rust tests without coverage
  rust-coverage       Run the exact 100 percent Rust coverage gate
  dup                 Run separate production, test, and documentation clone scans
  secrets             Scan files, the staged index, and Git history for secrets
  audit               Audit JavaScript and Rust dependencies
  build               Build release CLI and server binaries
  rust-build          Build release CLI and server binaries
  operator-acceptance Run filesystem and MinIO self-hosted acceptance
  gates-self-test     Prove hard gates reject isolated negative controls
  release-version     Enforce canonical CLI release versioning
  static              Run all non-coverage and non-build gates
  pre-commit          Check staged formatting, package policy, and secrets
  pre-push            Run the fast pre-push gate
  all                 Run every release-blocking gate
USAGE
}

case "${1:-}" in
  fmt | no-npm | lint | typecheck | contracts | test | test-fast | rust-coverage | dup | secrets | \
    audit | build | rust-build | operator-acceptance | gates-self-test | release-version)
    run_gate "$1"
    ;;
  static)
    run_static
    ;;
  pre-commit)
    run_pre_commit
    ;;
  pre-push)
    run_pre_push
    ;;
  all)
    run_all
    ;;
  -h | --help | help)
    usage
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
