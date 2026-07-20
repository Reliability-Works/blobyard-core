#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
# shellcheck source=scripts/lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

MODE="install"
if [[ "${1:-}" == "--check" ]]; then
  MODE="check"
elif [[ "$#" -ne 0 ]]; then
  printf 'Usage: %s [--check]\n' "$0" >&2
  exit 2
fi

blobyard_prepend_mise_shims
cd "$REPO_ROOT"
if blobyard_has_rust_workspace; then
  blobyard_prepend_rustup_toolchain
fi

blobyard_log_step "bootstrap" "Checking required tools"
blobyard_require_command git "Install Git with the system package manager."
blobyard_log_pass "git $(git --version | awk '{print $3}')"

blobyard_require_command node "Install Node 22.13 or newer through mise."
# shellcheck disable=SC2016
node -e '
const actual = process.versions.node.split(".").map(Number);
const minimum = [22, 13, 0];
const valid = actual.some((value, index) => value > minimum[index] && actual.slice(0, index).every((part, partIndex) => part === minimum[partIndex])) || actual.every((value, index) => value === minimum[index]);
if (!valid) {
  console.error(`ERROR: Node >=22.13.0 is required; detected ${process.versions.node}.`);
  process.exit(1);
}
'
blobyard_log_pass "node $(node --version)"

blobyard_require_command corepack "Use the Corepack bundled with Node."
blobyard_log_pass "corepack available"
blobyard_require_command pnpm "Run 'corepack enable' and activate the packageManager version."

EXPECTED_PNPM=$(node -e 'const value = require("./package.json").packageManager; process.stdout.write(value.split("@").at(-1));')
ACTUAL_PNPM=$(pnpm --version)
if [[ "$ACTUAL_PNPM" != "$EXPECTED_PNPM" ]]; then
  blobyard_die "pnpm $EXPECTED_PNPM is required by package.json; detected $ACTUAL_PNPM. Use Corepack to activate the pinned version."
fi
blobyard_log_pass "pnpm $ACTUAL_PNPM"

for required_tool in shellcheck shfmt actionlint gitleaks; do
  blobyard_require_command "$required_tool" "Install the repository quality tools documented in README.md."
  blobyard_log_pass "$required_tool available"
done

if blobyard_has_rust_workspace; then
  blobyard_require_command cargo "Install the Rust toolchain pinned by rust-toolchain.toml."
  blobyard_require_command rustc "Install the Rust toolchain pinned by rust-toolchain.toml."
  for cargo_tool in cargo-nextest cargo-llvm-cov cargo-deny cargo-audit; do
    blobyard_require_command "$cargo_tool" "Install the pinned Rust quality tools."
    blobyard_log_pass "$cargo_tool available"
  done
  if rustc --version | grep -Eq '(nightly|beta)'; then
    blobyard_die "a stable Rust toolchain is required; detected $(rustc --version)"
  fi
  blobyard_log_pass "$(rustc --version)"
else
  blobyard_log_skip "Rust workspace is not present yet"
fi

blobyard_log_step "bootstrap" "Checking package-manager policy"
"$SCRIPT_DIR/check-no-npm.sh"
"$SCRIPT_DIR/check-no-npm.sh" --self-test

if [[ "$MODE" == "check" ]]; then
  blobyard_log_pass "bootstrap prerequisite check complete"
  exit 0
fi

blobyard_log_step "bootstrap" "Installing workspace dependencies"
if blobyard_has_js_workspace; then
  if [[ -f pnpm-lock.yaml ]]; then
    pnpm install --frozen-lockfile
  else
    pnpm install
  fi
else
  blobyard_log_skip "pnpm workspace is not present yet"
fi

if blobyard_has_rust_workspace; then
  if [[ -f Cargo.lock ]]; then
    cargo fetch --locked
  else
    cargo fetch
  fi
fi

blobyard_log_step "bootstrap" "Installing repository hooks"
"$SCRIPT_DIR/install-git-hooks.sh"
blobyard_log_pass "bootstrap complete"
