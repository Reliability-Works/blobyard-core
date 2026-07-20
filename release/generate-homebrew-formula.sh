#!/usr/bin/env bash

set -euo pipefail

if (($# != 3)); then
  printf 'Usage: %s <release-manifest> <asset-directory> <output-file>\n' "$0" >&2
  exit 2
fi

manifest=$1
assets=$2
output=$3
version=$(jq -er '.version' "$manifest")
arm_archive=$(jq -er '.platforms[] | select(.key == "darwin-arm64") | .archive' "$manifest")
intel_archive=$(jq -er '.platforms[] | select(.key == "darwin-amd64") | .archive' "$manifest")
linux_arm_archive=$(jq -er '.platforms[] | select(.key == "linux-arm64") | .archive' "$manifest")
linux_intel_archive=$(jq -er '.platforms[] | select(.key == "linux-amd64") | .archive' "$manifest")

digest() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

arm_sha=$(digest "$assets/$arm_archive")
intel_sha=$(digest "$assets/$intel_archive")
linux_arm_sha=$(digest "$assets/$linux_arm_archive")
linux_intel_sha=$(digest "$assets/$linux_intel_archive")
base="https://releases.blobyard.com/v$version"

write_archives() {
  local arm_name=$1 arm_digest=$2 intel_name=$3 intel_digest=$4
  printf '    on_arm do\n'
  printf '      url "%s/%s"\n' "$base" "$arm_name"
  printf '      sha256 "%s"\n' "$arm_digest"
  printf '    end\n'
  printf '    on_intel do\n'
  printf '      url "%s/%s"\n' "$base" "$intel_name"
  printf '      sha256 "%s"\n' "$intel_digest"
  printf '    end\n'
}

{
  printf '# typed: strict\n'
  printf '# frozen_string_literal: true\n\n'
  printf '# Blobyard native CLI formula. Generated from the signed release manifest.\n'
  printf 'class Blobyard < Formula\n'
  printf '  desc "Secure artifact storage for developers"\n'
  printf '  homepage "https://blobyard.com"\n'
  printf '  version "%s"\n' "$version"
  printf '  license :cannot_represent\n\n'
  printf '  on_macos do\n'
  write_archives "$arm_archive" "$arm_sha" "$intel_archive" "$intel_sha"
  printf '  end\n'
  printf '  on_linux do\n'
  write_archives "$linux_arm_archive" "$linux_arm_sha" "$linux_intel_archive" "$linux_intel_sha"
  printf '  end\n\n'
  printf '  def install\n'
  printf '    bin.install "blobyard"\n'
  printf '  end\n\n'
  printf '  test do\n'
  printf '    system "/usr/bin/codesign", "--verify", "--deep", "--strict", bin/"blobyard" if OS.mac?\n'
  printf '    assert_match version.to_s, shell_output("#{bin}/blobyard --version")\n'
  printf '  end\n'
  printf 'end\n'
} >"$output"
