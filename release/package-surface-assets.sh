#!/usr/bin/env bash

set -euo pipefail

# shellcheck source=release/lib/artifacts.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/artifacts.sh"

if (($# != 3)); then
  printf 'Usage: %s <version> <source-revision> <output-directory>\n' "$0" >&2
  exit 2
fi

version=$1
source_revision=$2
output_directory=$3
validate_release_version "$version"
[[ $source_revision =~ ^[0-9a-f]{40}$ ]] || {
  printf 'Source revision must be a full lowercase Git commit SHA: %s\n' "$source_revision" >&2
  exit 2
}
for tool in jq pnpm tar; do
  require_release_tool "$tool"
done

workspace_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$RELEASE_ROOT/Cargo.toml" | head -n 1)
sdk_version=$(jq -er '.version' "$RELEASE_ROOT/sdk/typescript/package.json")
[[ $workspace_version == "$version" && $sdk_version == "$version" ]] || {
  printf 'Release, workspace, and SDK versions must match: %s / %s / %s\n' \
    "$version" "$workspace_version" "$sdk_version" >&2
  exit 1
}

mkdir -p "$output_directory"
output_directory=$(cd "$output_directory" && pwd)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

action_name=$(expand_asset_name "$(jq -er '.assets.actionBundle' "$ARTIFACT_DEFINITION")" "$version")
mkdir -p "$stage/action/.github/actions/upload" "$stage/action/scripts"
install -m 0644 "$RELEASE_ROOT/.github/actions/upload/action.yml" \
  "$stage/action/.github/actions/upload/action.yml"
install -m 0755 "$RELEASE_ROOT/.github/actions/upload/run.sh" \
  "$stage/action/.github/actions/upload/run.sh"
install -m 0755 "$RELEASE_ROOT/scripts/install.sh" "$stage/action/scripts/install.sh"
find "$stage/action" -exec touch -t 198001010000 {} +
COPYFILE_DISABLE=1 tar -cf - -C "$stage/action" \
  .github/actions/upload/action.yml \
  .github/actions/upload/run.sh \
  scripts/install.sh | gzip -n >"$output_directory/$action_name"

conformance_name=$(expand_asset_name \
  "$(jq -er '.assets.conformanceBundle' "$ARTIFACT_DEFINITION")" "$version")
cp -R "$RELEASE_ROOT/conformance" "$stage/conformance"
jq --arg version "$version" --arg sourceRevision "$source_revision" \
  '.coreVersion = $version | .sourceRevision = $sourceRevision' \
  "$stage/conformance/manifest.json" >"$stage/conformance/manifest.next.json"
mv "$stage/conformance/manifest.next.json" "$stage/conformance/manifest.json"
(
  cd "$stage/conformance"
  mapfile -t members < <(find . -type f ! -name SHA256SUMS -print | sed 's#^./##' | LC_ALL=C sort)
  : >SHA256SUMS
  for member in "${members[@]}"; do
    sha256sum "$member" >>SHA256SUMS
  done
)
find "$stage/conformance" -exec touch -t 198001010000 {} +
find "$stage/conformance" -type f -print | sed "s#^$stage/##" | LC_ALL=C sort \
  >"$stage/conformance-members.txt"
COPYFILE_DISABLE=1 tar -cf - -C "$stage" -T "$stage/conformance-members.txt" | gzip -n \
  >"$output_directory/$conformance_name"

sdk_name=$(expand_asset_name "$(jq -er '.assets.sdkPackage' "$ARTIFACT_DEFINITION")" "$version")
pnpm --dir "$RELEASE_ROOT/sdk/typescript" pack --pack-destination "$stage/sdk" >/dev/null
mapfile -t sdk_packages < <(find "$stage/sdk" -maxdepth 1 -type f -name '*.tgz' -print)
[[ ${#sdk_packages[@]} == 1 ]] || {
  printf 'SDK pack did not produce exactly one archive.\n' >&2
  exit 1
}
mv "${sdk_packages[0]}" "$output_directory/$sdk_name"

printf '%s\n' \
  "$output_directory/$action_name" \
  "$output_directory/$conformance_name" \
  "$output_directory/$sdk_name"
