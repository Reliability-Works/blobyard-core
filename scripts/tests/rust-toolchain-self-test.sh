#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/../.." && pwd)
# shellcheck source=scripts/lib/common.sh
source "$REPO_ROOT/scripts/lib/common.sh"

if ! grep -F 'components = ["clippy", "rustfmt", "llvm-tools-preview"]' \
  "$REPO_ROOT/rust-toolchain.toml" >/dev/null; then
  printf 'pinned Rust toolchain does not provide LLVM coverage tools\n' >&2
  exit 1
fi

if grep -E 'prepare_brew_llvm|LLVM_(COV|PROFDATA)|/opt/homebrew/opt/llvm' \
  "$REPO_ROOT/scripts/check.sh" >/dev/null; then
  printf 'Rust coverage gate overrides the pinned LLVM tools\n' >&2
  exit 1
fi

cd "$REPO_ROOT"
blobyard_prepend_rustup_toolchain
if [[ "$(command -v cargo)" != "$(rustup which cargo)" ]]; then
  printf 'repository checks do not resolve the pinned Cargo binary\n' >&2
  exit 1
fi

printf 'Rust toolchain self-test passed\n'
