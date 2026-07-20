#!/usr/bin/env bash

set -euo pipefail

if (($# != 2)); then
  printf 'Usage: %s <release-manifest> <asset-directory>\n' "$0" >&2
  exit 2
fi

manifest=$1
directory=$2
checksums=$(jq -er '.assets.checksums' "$manifest")
# shellcheck source=release/lib/artifacts.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/artifacts.sh"
mapfile -t files < <(release_asset_names "$manifest" | LC_ALL=C sort)

for file in "${files[@]}"; do
  if [[ ! -f $directory/$file ]]; then
    printf 'Release asset is missing before checksum generation: %s\n' "$file" >&2
    exit 1
  fi
done

(
  cd "$directory"
  : >"$checksums"
  for file in "${files[@]}"; do
    sha256sum "$file" >>"$checksums"
  done
)
