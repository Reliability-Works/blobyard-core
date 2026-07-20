#!/usr/bin/env bash

set -euo pipefail

action_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
repo_root=$(cd "$action_root/../../.." && pwd)
temporary=$(mktemp -d)
server_pid=

cleanup() {
  [[ -z $server_pid ]] || kill "$server_pid" 2>/dev/null || true
  rm -rf "$temporary"
}
trap cleanup EXIT HUP INT TERM

cli=${BLOBYARD_COMPILED_CLI_PATH:-"$repo_root/target/debug/blobyard"}
if [[ ! -x $cli ]]; then
  cargo build --manifest-path "$repo_root/Cargo.toml" --package blobyard-cli --locked
fi
[[ -x $cli ]] || {
  printf 'compiled Blobyard CLI is unavailable\n' >&2
  exit 1
}

port_file="$temporary/port"
BLOBYARD_TEST_PORT_FILE="$port_file" \
  node "$action_root/tests/fixtures/compiled-cli-server.mjs" >"$temporary/server.log" 2>&1 &
server_pid=$!
for _attempt in {1..100}; do
  [[ -s $port_file ]] && break
  kill -0 "$server_pid" 2>/dev/null || {
    printf 'compiled CLI test server exited early\n' >&2
    exit 1
  }
  sleep 0.05
done
[[ -s $port_file ]] || {
  printf 'compiled CLI test server did not become ready\n' >&2
  exit 1
}

fixture="$temporary/artifact.txt"
printf 'compiled action fixture\n' >"$fixture"
output="$temporary/output"
GITHUB_ACTION_PATH="$action_root" \
  GITHUB_OUTPUT="$output" \
  RUNNER_TEMP="$temporary/runner" \
  BLOBYARD_ACTION_API_URL="http://127.0.0.1:$(<"$port_file")/v1" \
  BLOBYARD_ACTION_WEB_YARD_ORIGIN="http://localhost:$(<"$port_file")" \
  BLOBYARD_ACTION_COMMENT=false \
  BLOBYARD_ACTION_EXPIRES=7d \
  BLOBYARD_ACTION_LOCAL_CLI="$cli" \
  BLOBYARD_ACTION_PATH="$fixture" \
  BLOBYARD_ACTION_PROJECT=demo \
  BLOBYARD_ACTION_SHARE=false \
  BLOBYARD_ACTION_TOKEN=scoped-compiled-fixture \
  BLOBYARD_ACTION_WORKSPACE=acme \
  "$action_root/run.sh" 2>"$temporary/action.log"

grep -Fxq 'uri=blobyard://acme/demo/artifact.txt?version=7' "$output"
grep -Fxq 'version=7' "$output"
grep -Fxq 'share-url=' "$output"
if grep -Fq 'scoped-compiled-fixture' "$temporary/action.log"; then
  printf 'compiled action test exposed its bearer fixture\n' >&2
  exit 1
fi
printf 'compiled CLI action self-test passed\n'
