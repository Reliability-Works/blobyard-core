#!/usr/bin/env bash

set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$root/Cargo.toml" | head -n 1)
source_revision=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa

first="$temporary/first"
second="$temporary/second"
mkdir "$first" "$second"
"$root/release/package-surface-assets.sh" "$version" "$source_revision" "$first" >/dev/null
"$root/release/package-surface-assets.sh" "$version" "$source_revision" "$second" >/dev/null
"$root/release/generate-release-manifest.sh" \
  "$version" "$source_revision" "$first/blobyard-release-manifest.json"

for asset in \
  "blobyard-action-$version.tar.gz" \
  "blobyard-conformance-$version.tar.gz" \
  "blobyard-sdk-$version.tgz"; do
  cmp "$first/$asset" "$second/$asset"
done

action="$first/blobyard-action-$version.tar.gz"
mapfile -t action_members < <(tar -tzf "$action" | sed 's#^\./##' | LC_ALL=C sort)
expected_action_members=(
  .github/actions/upload/action.yml
  .github/actions/upload/run.sh
  scripts/install.sh
)
[[ ${action_members[*]} == "${expected_action_members[*]}" ]]

sdk="$first/blobyard-sdk-$version.tgz"
tar -xzf "$sdk" -C "$temporary"
jq -e --arg version "$version" \
  '.name == "@blobyard/sdk" and .version == $version and .private == true' \
  "$temporary/package/package.json" >/dev/null
[[ -f $temporary/package/src/index.mjs ]]
[[ -f $temporary/package/src/index.d.mts ]]

conformance="$first/blobyard-conformance-$version.tar.gz"
tar -xzf "$conformance" -C "$temporary"
jq -e --arg version "$version" --arg sourceRevision "$source_revision" \
  '.coreVersion == $version and .sourceRevision == $sourceRevision' \
  "$temporary/conformance/manifest.json" >/dev/null
(
  cd "$temporary/conformance"
  sha256sum --check --strict SHA256SUMS >/dev/null
)

"$root/release/verify-surface-assets.sh" "$first" >/dev/null
mkdir "$temporary/tampered-action"
tar -xzf "$first/blobyard-action-$version.tar.gz" -C "$temporary/tampered-action"
printf 'tamper\n' >"$temporary/tampered-action/extra.txt"
tar -czf "$first/blobyard-action-$version.tar.gz" -C "$temporary/tampered-action" .
if "$root/release/verify-surface-assets.sh" "$first" >/dev/null 2>&1; then
  printf 'Tampered Action bundle passed surface verification.\n' >&2
  exit 1
fi

printf 'surface asset self-test passed\n'
