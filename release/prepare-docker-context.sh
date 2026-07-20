#!/usr/bin/env bash

set -euo pipefail

if (($# < 2 || $# > 3)); then
  printf 'Usage: %s <asset-directory> <docker-context-directory> [cli|server]\n' "$0" >&2
  exit 2
fi

assets=$1
context=$2
surface=${3:-cli}
manifest="$assets/blobyard-release-manifest.json"
mkdir -p "$context/bin/amd64" "$context/bin/arm64"

case "$surface" in
  cli)
    manifest_key=platforms
    executable=blobyard
    ;;
  server)
    manifest_key=serverPlatforms
    executable=blobyard-server
    ;;
  *)
    printf 'Unsupported Docker image surface: %s\n' "$surface" >&2
    exit 2
    ;;
esac

for arch in amd64 arm64; do
  archive=$(jq -er --arg arch "$arch" --arg key "$manifest_key" \
    '.[$key][] | select(.os == "linux" and .arch == $arch) | .archive' "$manifest")
  expected=$(awk -v name="$archive" '$2 == name {print $1}' "$assets/SHA256SUMS")
  actual=$(sha256sum "$assets/$archive" | awk '{print $1}')
  [[ -n $expected && $actual == "$expected" ]] || {
    printf 'Docker source archive checksum failed: %s\n' "$archive" >&2
    exit 1
  }
  [[ $(tar -tzf "$assets/$archive") == "$executable" ]] || {
    printf 'Docker source archive contains unexpected paths: %s\n' "$archive" >&2
    exit 1
  }
  tar -xzf "$assets/$archive" -C "$context/bin/$arch" "$executable"
done

(
  cd "$context"
  sha256sum "bin/amd64/$executable" "bin/arm64/$executable" >SHA256SUMS
)
