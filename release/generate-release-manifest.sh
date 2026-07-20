#!/usr/bin/env bash

set -euo pipefail

# shellcheck source=release/lib/artifacts.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/artifacts.sh"

if (($# != 3)); then
  printf 'Usage: %s <version> <source-revision> <output-file>\n' "$0" >&2
  exit 2
fi

version=$1
source_revision=$2
output=$3
validate_release_version "$version"
if [[ ! $source_revision =~ ^[0-9a-f]{40}$ ]]; then
  printf 'Source revision must be a full lowercase Git commit SHA: %s\n' "$source_revision" >&2
  exit 2
fi
require_release_tool jq

jq --arg version "$version" --arg sourceRevision "$source_revision" '
  def expand:
    gsub("\\{version\\}"; $version);
  {
    schemaVersion,
    repository,
    binary,
    version: $version,
    sourceRevision: $sourceRevision,
    releaseManifest,
    signing,
    assets: (.assets | with_entries(.value |= expand)),
    containers,
    platforms: [
      .platforms[]
      | . as $platform
      | .archive |= (expand | gsub("\\{target\\}"; $platform.target))
    ],
    serverPlatforms: [
      .serverPlatforms[]
      | . as $platform
      | .archive |= (expand | gsub("\\{target\\}"; $platform.target))
    ]
  }
' "$ARTIFACT_DEFINITION" >"$output"

jq -e '
  .schemaVersion == 1 and
  (.platforms | length == 4) and
  (.serverPlatforms | length == 2) and
  ([.containers[].surface] == ["cli", "server"])
' "$output" >/dev/null
