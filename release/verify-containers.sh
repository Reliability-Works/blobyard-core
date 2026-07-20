#!/usr/bin/env bash

set -euo pipefail

# shellcheck source=release/lib/artifacts.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/artifacts.sh"

if (($# != 1)); then
  printf 'Usage: %s <asset-directory>\n' "$0" >&2
  exit 2
fi

directory=$1
release_manifest="$directory/blobyard-release-manifest.json"
for tool in jq gh cosign; do
  require_release_tool "$tool"
done

container_asset=$(jq -er '.assets.containerImages' "$release_manifest")
container_manifest="$directory/$container_asset"
repository=$(jq -er '.repository' "$release_manifest")
version=$(jq -er '.version' "$release_manifest")
source_revision=$(jq -er '.sourceRevision' "$release_manifest")
workflow=$(jq -er '.signing.workflow' "$release_manifest")
issuer=$(jq -er '.signing.oidcIssuer' "$release_manifest")
signing_ref=$(jq -er '.signing.ref' "$release_manifest")
identity=$(release_workflow_identity "$repository" "$workflow" "$signing_ref")

jq -e \
  --arg repository "$repository" \
  --arg version "$version" \
  --arg sourceRevision "$source_revision" '
    .schemaVersion == 1 and
    .repository == $repository and
    .version == $version and
    .sourceRevision == $sourceRevision and
    ([.images[].surface] == ["cli", "server"]) and
    all(.images[];
      (.image | test("^ghcr\\.io/reliability-works/blobyard-core-(cli|server)$")) and
      (.digest | test("^sha256:[0-9a-f]{64}$")) and
      .reference == (.image + "@" + .digest)
    )
  ' "$container_manifest" >/dev/null

while IFS= read -r reference; do
  cosign verify \
    --certificate-identity "$identity" \
    --certificate-oidc-issuer "$issuer" \
    --certificate-github-workflow-sha "$source_revision" \
    "$reference" >/dev/null
  gh attestation verify "oci://$reference" \
    --repo "$repository" \
    --cert-identity "$identity" \
    --source-ref "$signing_ref" \
    --source-digest "$source_revision" \
    --signer-digest "$source_revision" >/dev/null
done < <(jq -er '.images[].reference' "$container_manifest")

printf 'Verified %s signed private container images for Blobyard %s.\n' \
  "$(jq '.images | length' "$container_manifest")" "$version"
