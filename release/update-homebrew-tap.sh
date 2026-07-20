#!/usr/bin/env bash

set -euo pipefail

if (($# != 2)); then
  printf 'Usage: %s <formula> <tap-directory>\n' "$0" >&2
  exit 2
fi

formula=$1
tap=$2
if [[ ! -f $formula || ! -d $tap/.git ]]; then
  printf 'Formula or checked-out Homebrew tap is unavailable.\n' >&2
  exit 1
fi

mkdir -p "$tap/Formula"
temporary=$(mktemp "$tap/Formula/.blobyard.rb.XXXXXX")
trap 'rm -f "$temporary"' EXIT
install -m 0644 "$formula" "$temporary"
ruby -c "$temporary" >/dev/null
mv -f "$temporary" "$tap/Formula/blobyard.rb"
printf 'Updated %s/Formula/blobyard.rb.\n' "$tap"
