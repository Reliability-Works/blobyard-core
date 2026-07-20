#!/usr/bin/env bash

set -euo pipefail

# shellcheck source=release/lib/artifacts.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/artifacts.sh"

if (($# != 1)); then
  printf 'Usage: %s <asset-directory>\n' "$0" >&2
  exit 2
fi

directory=$1
manifest="$directory/blobyard-release-manifest.json"
for tool in jq tar; do
  require_release_tool "$tool"
done
if command -v sha256sum >/dev/null 2>&1; then
  checksum_command=(sha256sum --check --strict)
else
  require_release_tool shasum
  checksum_command=(shasum -a 256 --check)
fi

version=$(jq -er '.version' "$manifest")
source_revision=$(jq -er '.sourceRevision' "$manifest")
action=$(jq -er '.assets.actionBundle' "$manifest")
conformance=$(jq -er '.assets.conformanceBundle' "$manifest")
sdk=$(jq -er '.assets.sdkPackage' "$manifest")
for asset in "$action" "$conformance" "$sdk"; do
  validate_asset_name "$asset"
  [[ -s $directory/$asset ]] || {
    printf 'Packed surface asset is missing or empty: %s\n' "$asset" >&2
    exit 1
  }
done

temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT

mapfile -t action_members < <(tar -tzf "$directory/$action")
expected_action_members=(
  .github/actions/upload/action.yml
  .github/actions/upload/run.sh
  scripts/install.sh
)
[[ ${action_members[*]} == "${expected_action_members[*]}" ]] || {
  printf 'Action bundle does not contain the exact production file set.\n' >&2
  exit 1
}
tar -xzf "$directory/$action" -C "$temporary"
[[ -x $temporary/.github/actions/upload/run.sh && -x $temporary/scripts/install.sh ]] || {
  printf 'Action bundle scripts are not executable.\n' >&2
  exit 1
}

mapfile -t sdk_members < <(tar -tzf "$directory/$sdk" | LC_ALL=C sort)
expected_sdk_members=(
  package/LICENSE
  package/README.md
  package/package.json
  package/src/index.d.mts
  package/src/index.mjs
  package/src/operations.generated.d.mts
  package/src/operations.generated.mjs
)
[[ ${sdk_members[*]} == "${expected_sdk_members[*]}" ]] || {
  printf 'SDK package does not contain the exact source and type contract.\n' >&2
  exit 1
}
tar -xzf "$directory/$sdk" -C "$temporary"
jq -e --arg version "$version" '
  .name == "@blobyard/sdk" and
  .version == $version and
  .private == true and
  .license == "Apache-2.0"
' "$temporary/package/package.json" >/dev/null || {
  printf 'SDK package identity does not match the release.\n' >&2
  exit 1
}

while IFS= read -r member; do
  [[ $member =~ ^conformance/[A-Za-z0-9._/-]+$ && $member != *"/../"* && $member != */.. ]] || {
    printf 'Conformance bundle contains an unsafe path: %s\n' "$member" >&2
    exit 1
  }
done < <(tar -tzf "$directory/$conformance")
tar -xzf "$directory/$conformance" -C "$temporary"
[[ -z $(find "$temporary/conformance" -type l -print -quit) ]] || {
  printf 'Conformance bundle contains a symbolic link.\n' >&2
  exit 1
}
jq -e --arg version "$version" --arg sourceRevision "$source_revision" '
  .schemaVersion == 1 and
  .coreVersion == $version and
  .sourceRevision == $sourceRevision and
  (.members | type == "array" and length > 0) and
  all(.members[];
    (.path | test("^[A-Za-z0-9._/-]+$")) and
    (.sha256 | test("^[0-9a-f]{64}$")) and
    (.size | type == "number" and floor == . and . >= 0)
  )
' "$temporary/conformance/manifest.json" >/dev/null || {
  printf 'Conformance manifest identity does not match the release.\n' >&2
  exit 1
}
expected_conformance_members=$(jq -r \
  '["conformance/SHA256SUMS", "conformance/manifest.json"] + [.members[].path | "conformance/" + .] | unique | .[]' \
  "$temporary/conformance/manifest.json" | LC_ALL=C sort)
actual_conformance_members=$(tar -tzf "$directory/$conformance" | LC_ALL=C sort)
[[ $actual_conformance_members == "$expected_conformance_members" ]] || {
  printf 'Conformance bundle does not contain the exact manifest member set.\n' >&2
  exit 1
}
(
  cd "$temporary/conformance"
  "${checksum_command[@]}" SHA256SUMS >/dev/null
)

printf 'Verified packed Action, SDK, and conformance assets for Blobyard %s.\n' "$version"
