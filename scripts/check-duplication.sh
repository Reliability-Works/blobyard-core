#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
# shellcheck source=scripts/lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

blobyard_prepend_mise_shims
cd "$REPO_ROOT"
blobyard_require_command pnpm "Run scripts/bootstrap.sh first."

TEMP_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/blobyard-dup.XXXXXX")
trap 'rm -rf "$TEMP_ROOT"' EXIT

is_excluded() {
  case "/$1" in
    */node_modules/* | */coverage/* | */dist/* | */out/* | */target/* | */report/* | */scripts/tests/fixtures/* | */pnpm-lock.yaml | */Cargo.lock | *.lock | *.snap)
      return 0
      ;;
  esac
  return 1
}

is_test_source() {
  case "/$1" in
    */test/* | */tests/* | */type-tests/* | *.test.ts | *.test.tsx | *.test.js | *.test.jsx | *.spec.ts | *.spec.tsx | *.spec.js | *.spec.jsx | *_test.rs | *_tests.rs)
      return 0
      ;;
  esac
  return 1
}

category_for() {
  local file="$1"

  if is_test_source "$file"; then
    case "$file" in
      *.ts | *.tsx | *.js | *.jsx | *.mjs | *.cjs | *.rs | *.sh | *.bash | *.zsh) printf 'tests' ;;
    esac
    return
  fi

  case "$file" in
    *.ts | *.tsx | *.js | *.jsx | *.mjs | *.cjs | *.rs | *.sh | *.bash | *.zsh | *.css) printf 'production' ;;
    *.md | *.mdx | *.json | *.yaml | *.yml | *.toml) printf 'documentation' ;;
  esac
}

stage_files() {
  local file
  local category
  local destination

  while IFS= read -r -d '' file; do
    if [[ ! -f "$file" ]]; then
      continue
    fi
    if is_excluded "$file"; then
      continue
    fi
    category=$(category_for "$file")
    if [[ -z "$category" ]]; then
      continue
    fi
    destination="$TEMP_ROOT/$category/$file"
    mkdir -p "$(dirname "$destination")"
    cp "$file" "$destination"
  done < <(git ls-files --cached --others --exclude-standard -z)
}

run_scan() {
  local category="$1"
  local min_lines="$2"
  local min_tokens="$3"
  local directory="$TEMP_ROOT/$category"

  blobyard_log_step "dup" "$category source (minLines=$min_lines, minTokens=$min_tokens)"
  if [[ ! -d "$directory" ]] || ! find "$directory" -type f -print -quit | grep -q .; then
    blobyard_log_skip "no $category files"
    return
  fi

  pnpm exec jscpd "$directory" \
    --config "$REPO_ROOT/.jscpd.json" \
    --min-lines "$min_lines" \
    --min-tokens "$min_tokens" \
    --no-tips \
    --output "$TEMP_ROOT/$category-report" \
    --reporters console \
    --threshold 0
}

stage_files
run_scan production 5 50
run_scan tests 8 70
run_scan documentation 10 100
