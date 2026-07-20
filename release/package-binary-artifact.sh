#!/usr/bin/env bash

set -euo pipefail

# shellcheck source=release/lib/artifacts.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/artifacts.sh"

if (($# != 5)); then
  printf 'Usage: %s <cli|server> <version> <target> <binary> <output-directory>\n' "$0" >&2
  exit 2
fi

kind=$1
version=$2
target=$3
binary=$4
output_directory=$5
validate_release_version "$version"

case $kind in
  cli)
    archive=$(archive_for_target "$version" "$target")
    executable=$(executable_for_target "$target")
    ;;
  server)
    archive=$(server_archive_for_target "$version" "$target")
    executable=$(server_executable_for_target "$target")
    ;;
  *)
    printf 'Release binary kind must be cli or server: %s\n' "$kind" >&2
    exit 2
    ;;
esac

if [[ ! -f $binary ]]; then
  printf 'Release binary does not exist: %s\n' "$binary" >&2
  exit 1
fi

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$output_directory"
output_directory=$(cd "$output_directory" && pwd)
install -m 0755 "$binary" "$stage/$executable"
TZ=UTC touch -t 198001010000 "$stage/$executable"

case $archive in
  *.tar.gz)
    COPYFILE_DISABLE=1 tar -cf - -C "$stage" "$executable" | gzip -n >"$output_directory/$archive"
    ;;
  *)
    printf 'Unsupported release archive format: %s\n' "$archive" >&2
    exit 1
    ;;
esac

printf '%s\n' "$output_directory/$archive"
