#!/usr/bin/env bash

set -euo pipefail

# shellcheck source=release/lib/artifacts.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/artifacts.sh"

if (($# != 4)); then
  printf 'Usage: %s <version> <source-revision> <metadata-directory> <output-file>\n' "$0" >&2
  exit 2
fi

version=$1
source_revision=$2
metadata_directory=$3
output=$4
validate_release_version "$version"
[[ $source_revision =~ ^[0-9a-f]{40}$ ]] || {
  printf 'Source revision must be a full lowercase Git commit SHA: %s\n' "$source_revision" >&2
  exit 2
}
require_release_tool jq

images='[]'
while IFS=$'\t' read -r surface image; do
  metadata="$metadata_directory/$surface.json"
  [[ -s $metadata ]] || {
    printf 'Container metadata is missing: %s\n' "$metadata" >&2
    exit 1
  }
  digest=$(jq -er '.digest | select(test("^sha256:[0-9a-f]{64}$"))' "$metadata")
  actual_image=$(jq -er '.image' "$metadata")
  [[ $actual_image == "$image" ]] || {
    printf 'Container image does not match the release definition: %s\n' "$surface" >&2
    exit 1
  }
  images=$(jq -cn \
    --argjson images "$images" \
    --arg surface "$surface" \
    --arg image "$image" \
    --arg digest "$digest" \
    '$images + [{surface: $surface, image: $image, digest: $digest, reference: ($image + "@" + $digest)}]')
done < <(jq -r '.containers[] | [.surface, .image] | @tsv' "$ARTIFACT_DEFINITION")

jq -n \
  --arg repository "$(jq -er '.repository' "$ARTIFACT_DEFINITION")" \
  --arg version "$version" \
  --arg sourceRevision "$source_revision" \
  --argjson images "$images" \
  '{
    schemaVersion: 1,
    repository: $repository,
    version: $version,
    sourceRevision: $sourceRevision,
    images: $images
  }' >"$output"
