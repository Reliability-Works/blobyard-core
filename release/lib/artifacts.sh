#!/usr/bin/env bash

set -euo pipefail

RELEASE_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
ARTIFACT_DEFINITION=${ARTIFACT_DEFINITION:-"$RELEASE_ROOT/release/artifacts.json"}

require_release_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'Required release tool is unavailable: %s\n' "$1" >&2
    return 1
  fi
}

validate_release_version() {
  if [[ ! $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]]; then
    printf 'Release version must be semantic and omit the v prefix: %s\n' "$1" >&2
    return 1
  fi
}

validate_asset_name() {
  if [[ -z $1 || $1 == */* || $1 == *\\* || $1 == .* || $1 == *[^A-Za-z0-9._-]* ]]; then
    printf 'Release asset name is unsafe: %s\n' "$1" >&2
    return 1
  fi
}

release_workflow_identity() {
  local repository=$1 workflow=$2 ref=$3
  printf 'https://github.com/%s/%s@%s\n' "$repository" "$workflow" "$ref"
}

expand_asset_name() {
  local template=$1 version=$2 target=${3:-}
  template=${template//\{version\}/$version}
  printf '%s\n' "${template//\{target\}/$target}"
}

archive_for_target() {
  local version=$1 target=$2 template
  template=$(jq -er --arg target "$target" '.platforms[] | select(.target == $target) | .archive' "$ARTIFACT_DEFINITION")
  expand_asset_name "$template" "$version" "$target"
}

executable_for_target() {
  jq -er --arg target "$1" '.platforms[] | select(.target == $target) | .executable' "$ARTIFACT_DEFINITION"
}

server_archive_for_target() {
  local version=$1 target=$2 template
  template=$(jq -er --arg target "$target" '.serverPlatforms[] | select(.target == $target) | .archive' "$ARTIFACT_DEFINITION")
  expand_asset_name "$template" "$version" "$target"
}

server_executable_for_target() {
  jq -er --arg target "$1" '.serverPlatforms[] | select(.target == $target) | .executable' "$ARTIFACT_DEFINITION"
}

release_asset_names() {
  jq -er '
    [
      .releaseManifest,
      .assets.actionBundle,
      .assets.conformanceBundle,
      .assets.containerImages,
      .assets.sbom,
      .assets.sdkPackage,
      .assets.homebrewFormula
    ] + [.platforms[].archive] + [.serverPlatforms[].archive] | .[]
  ' "$1"
}
