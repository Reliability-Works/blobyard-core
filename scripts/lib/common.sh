#!/usr/bin/env bash

blobyard_prepend_mise_shims() {
  local shims_dir="${HOME:-}/.local/share/mise/shims"

  if [[ -n "${HOME:-}" && -d "$shims_dir" ]]; then
    export PATH="$shims_dir:$PATH"
  fi
}

blobyard_prepend_rustup_toolchain() {
  local pinned_bin
  local pinned_cargo

  blobyard_require_command rustup "Install rustup and the toolchain pinned by rust-toolchain.toml."
  if ! pinned_cargo=$(rustup which cargo); then
    blobyard_die "The Rust toolchain pinned by rust-toolchain.toml is unavailable."
    return 1
  fi
  pinned_bin=$(dirname "$pinned_cargo")
  export PATH="$pinned_bin:$PATH"
}

blobyard_log_step() {
  printf '\n[%s] %s\n' "$1" "$2"
}

blobyard_log_pass() {
  printf 'PASS: %s\n' "$1"
}

blobyard_log_skip() {
  printf 'SKIP: %s\n' "$1"
}

blobyard_die() {
  printf 'ERROR: %s\n' "$1" >&2
  return 1
}

blobyard_require_command() {
  local command_name="$1"
  local install_hint="$2"

  if ! command -v "$command_name" >/dev/null 2>&1; then
    blobyard_die "$command_name is required. $install_hint"
  fi
}

blobyard_has_js_workspace() {
  [[ -f package.json && -f pnpm-workspace.yaml ]]
}

blobyard_has_rust_workspace() {
  [[ -f Cargo.toml ]]
}
