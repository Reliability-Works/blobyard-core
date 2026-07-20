#!/usr/bin/env bash

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
workflow="$repo_root/.github/workflows/release.yml"

if grep -Fq 'git fetch origin main' "$workflow"; then
  printf '%s\n' \
    'Release workflow performs an unauthenticated fetch after checkout removes credentials.' >&2
  exit 1
fi

grep -Fq "git rev-parse --verify 'refs/remotes/origin/main^{commit}'" "$workflow"
grep -Fq "git merge-base --is-ancestor \"\$COMMIT_SHA\" refs/remotes/origin/main" "$workflow"

prerequisites_job=$(sed -n '/^  prerequisites:/,/^  macos:/p' "$workflow")
grep -Fq "gh repo view \"\$GITHUB_REPOSITORY\"" <<<"$prerequisites_job"
grep -Fq '== "PUBLIC"' <<<"$prerequisites_job"
grep -Fq "gh release view \"\$RELEASE_TAG\" --repo \"\$GITHUB_REPOSITORY\" --json isDraft" \
  <<<"$prerequisites_job"
grep -Fq "gh release edit \"\$RELEASE_TAG\" \\" <<<"$prerequisites_job"
grep -Fq -- "--target \"\$COMMIT_SHA\"" <<<"$prerequisites_job"
grep -Fq "gh release create \"\$RELEASE_TAG\" \\" <<<"$prerequisites_job"
grep -Fq -- "--repo \"\$GITHUB_REPOSITORY\"" <<<"$prerequisites_job"

linux_job=$(sed -n '/^  linux:/,/^  assemble:/p' "$workflow")
server_build="cargo build --locked --release --target \"\${{ matrix.target }}\" --package blobyard-server"
server_smoke="output=\$(\"target/\${{ matrix.target }}/release/blobyard-server\" --version)"
grep -Fq -- "$server_build" <<<"$linux_job"
grep -Fq -- "$server_smoke" <<<"$linux_job"
build_line=$(grep -nF -- "$server_build" <<<"$linux_job" | cut -d: -f1)
smoke_line=$(grep -nF -- "$server_smoke" <<<"$linux_job" | cut -d: -f1)
[[ $build_line -lt $smoke_line ]]

containers_job=$(sed -n '/^  containers:/,/^  verify:/p' "$workflow")
prepare_context_command="release/prepare-docker-context.sh dist packaging/docker \"\$SURFACE\""
grep -Fq 'dockerfile: deploy/docker/server-release.Dockerfile' <<<"$containers_job"
grep -Fq 'name: Prepare verified image context' <<<"$containers_job"
grep -Fq "$prepare_context_command" <<<"$containers_job"
if grep -Fq "if: \${{ matrix.surface == 'cli' }}" <<<"$containers_job"; then
  printf 'Release workflow only prepares a verified CLI image context.\n' >&2
  exit 1
fi

stage_job=$(sed -n '/^  stage-release:/,$p' "$workflow")
grep -Fq "gh repo view \"\$GITHUB_REPOSITORY\"" <<<"$stage_job"
grep -Fq '== "PUBLIC"' <<<"$stage_job"
grep -Fq "gh release view \"\$RELEASE_TAG\" --repo \"\$GITHUB_REPOSITORY\"" <<<"$stage_job"
grep -Fq "gh release upload \"\$RELEASE_TAG\" \"\${assets[@]}\" --clobber \\" <<<"$stage_job"
grep -Fq -- "--repo \"\$GITHUB_REPOSITORY\"" <<<"$stage_job"

printf 'release workflow self-test passed\n'
