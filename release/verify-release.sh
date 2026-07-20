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
for tool in jq gh cosign; do
  require_release_tool "$tool"
done
if command -v sha256sum >/dev/null 2>&1; then
  sha256_file() { sha256sum "$1" | awk '{print $1}'; }
else
  require_release_tool shasum
  sha256_file() { shasum -a 256 "$1" | awk '{print $1}'; }
fi

[[ -f $manifest ]] || {
  printf 'Release manifest is missing: %s\n' "$manifest" >&2
  exit 1
}

repository=$(jq -er '.repository' "$manifest")
version=$(jq -er '.version' "$manifest")
checksums=$(jq -er '.assets.checksums' "$manifest")
signature=$(jq -er '.assets.checksumsSignature' "$manifest")
provenance=$(jq -er '.assets.provenance' "$manifest")
workflow=$(jq -er '.signing.workflow' "$manifest")
issuer=$(jq -er '.signing.oidcIssuer' "$manifest")
signing_ref=$(jq -r '.signing.ref // empty' "$manifest")
source_revision=$(jq -r '.sourceRevision // empty' "$manifest")
expected_repository=$(jq -er '.repository' "$ARTIFACT_DEFINITION")
expected_workflow=$(jq -er '.signing.workflow' "$ARTIFACT_DEFINITION")
expected_issuer=$(jq -er '.signing.oidcIssuer' "$ARTIFACT_DEFINITION")
expected_signing_ref=$(jq -er '.signing.ref' "$ARTIFACT_DEFINITION")

validate_release_version "$version"
[[ $repository == "$expected_repository" ]] || {
  printf 'Release manifest repository is not Blobyard.\n' >&2
  exit 1
}
[[ $workflow == "$expected_workflow" && $issuer == "$expected_issuer" ]] || {
  printf 'Release manifest signing identity is not trusted.\n' >&2
  exit 1
}
if [[ -n $signing_ref || -n $source_revision ]]; then
  [[ $signing_ref == "$expected_signing_ref" && $source_revision =~ ^[0-9a-f]{40}$ ]] || {
    printf 'Release manifest source identity is not trusted.\n' >&2
    exit 1
  }
else
  signing_ref="refs/tags/v$version"
fi

assets=()
while IFS= read -r asset; do
  assets+=("$asset")
done < <(release_asset_names "$manifest")
for asset in "$checksums" "$signature" "$provenance" "${assets[@]}"; do
  validate_asset_name "$asset"
  [[ -s $directory/$asset ]] || {
    printf 'Release asset is missing or empty: %s\n' "$asset" >&2
    exit 1
  }
done

identity=$(release_workflow_identity "$repository" "$workflow" "$signing_ref")
if [[ -n $source_revision ]]; then
  cosign verify-blob \
    --bundle "$directory/$signature" \
    --certificate-identity "$identity" \
    --certificate-oidc-issuer "$issuer" \
    --certificate-github-workflow-sha "$source_revision" \
    "$directory/$checksums" >/dev/null
else
  cosign verify-blob \
    --bundle "$directory/$signature" \
    --certificate-identity "$identity" \
    --certificate-oidc-issuer "$issuer" \
    "$directory/$checksums" >/dev/null
fi

verify_provenance() {
  if [[ -n $source_revision ]]; then
    gh attestation verify "$1" \
      --repo "$repository" \
      --bundle "$directory/$provenance" \
      --cert-identity "$identity" \
      --source-ref "$signing_ref" \
      --source-digest "$source_revision" \
      --signer-digest "$source_revision" >/dev/null
  else
    gh attestation verify "$1" \
      --repo "$repository" \
      --bundle "$directory/$provenance" \
      --cert-identity "$identity" \
      --source-ref "$signing_ref" >/dev/null
  fi
}

verify_provenance "$directory/$checksums"
while IFS= read -r line; do
  [[ $line =~ ^[0-9a-f]{64}\ \ [A-Za-z0-9._-]+$ ]] || {
    printf 'Checksum manifest contains an invalid entry.\n' >&2
    exit 1
  }
done <"$directory/$checksums"
expected_checksum_assets=$(printf '%s\n' "${assets[@]}" | LC_ALL=C sort)
actual_checksum_assets=$(awk '{print $2}' "$directory/$checksums" | LC_ALL=C sort)
[[ $actual_checksum_assets == "$expected_checksum_assets" ]] || {
  printf 'Checksum manifest does not contain the exact release asset set.\n' >&2
  exit 1
}
while read -r expected asset; do
  actual=$(sha256_file "$directory/$asset")
  [[ $actual == "$expected" ]] || {
    printf 'Checksum verification failed: %s\n' "$asset" >&2
    exit 1
  }
done <"$directory/$checksums"

for asset in "${assets[@]}"; do
  verify_provenance "$directory/$asset"
done

printf 'Verified %s release assets for Blobyard %s.\n' "${#assets[@]}" "$version"
