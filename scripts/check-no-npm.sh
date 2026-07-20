#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

if [[ "${1:-}" == "--self-test" ]]; then
  exec "$SCRIPT_DIR/tests/check-no-npm-self-test.sh"
fi

cd "$REPO_ROOT"
exec node "$SCRIPT_DIR/check-no-npm.mjs" "$@"
