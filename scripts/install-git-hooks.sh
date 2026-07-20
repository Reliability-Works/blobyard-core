#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

cd "$REPO_ROOT"
if [[ ! -d .git ]]; then
  printf 'ERROR: %s is not a Git working tree.\n' "$REPO_ROOT" >&2
  exit 1
fi

chmod +x .githooks/pre-commit .githooks/pre-push
git config --local core.hooksPath .githooks

if [[ "$(git config --local --get core.hooksPath)" != ".githooks" ]]; then
  printf 'ERROR: failed to activate .githooks.\n' >&2
  exit 1
fi

printf 'Git hooks active at .githooks.\n'
