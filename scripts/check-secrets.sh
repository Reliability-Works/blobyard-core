#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
# shellcheck source=scripts/lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

blobyard_prepend_mise_shims
blobyard_require_command git "Install Git before running secret checks."
blobyard_require_command gitleaks "Run scripts/bootstrap.sh first."

CONFIG_FILE="$REPO_ROOT/.gitleaks.toml"

scan_working_tree() (
  local file
  local source_path
  local staged_path
  local temp_root

  temp_root=$(mktemp -d "${TMPDIR:-/tmp}/blobyard-secrets.XXXXXX")
  trap 'rm -rf "$temp_root"' EXIT

  while IFS= read -r -d '' file; do
    source_path="$REPO_ROOT/$file"
    staged_path="$temp_root/tree/$file"
    if [[ -L "$source_path" ]]; then
      mkdir -p "$(dirname "$staged_path")"
      readlink "$source_path" >"$staged_path"
    elif [[ -f "$source_path" ]]; then
      mkdir -p "$(dirname "$staged_path")"
      cp "$source_path" "$staged_path"
    fi
  done < <(git -C "$REPO_ROOT" ls-files --cached --others --exclude-standard -z)

  mkdir -p "$temp_root/tree"
  gitleaks dir --redact --no-banner --config "$CONFIG_FILE" "$temp_root/tree"
)

scan_staged() {
  (
    cd "$REPO_ROOT"
    gitleaks git --pre-commit --staged --redact --no-banner --config "$CONFIG_FILE" .
  )
}

scan_history() {
  (
    cd "$REPO_ROOT"
    gitleaks git --redact --no-banner --config "$CONFIG_FILE" .
  )
}

scan_all() {
  scan_working_tree
  scan_staged
  scan_history
}

case "${1:-}" in
  working-tree)
    scan_working_tree
    ;;
  staged)
    scan_staged
    ;;
  history)
    scan_history
    ;;
  all)
    scan_all
    ;;
  *)
    printf 'Usage: scripts/check-secrets.sh <working-tree|staged|history|all>\n' >&2
    exit 2
    ;;
esac
