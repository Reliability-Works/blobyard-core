#!/bin/sh

set -eu

repository=Reliability-Works/blobyard-core
public_release_origin=https://releases.blobyard.com
release_manifest_name=blobyard-release-manifest.json
signing_workflow=.github/workflows/release.yml
signing_issuer=https://token.actions.githubusercontent.com
trusted_signing_ref=refs/heads/main
version=latest
install_dir=${HOME:+"$HOME/.local/bin"}
dry_run=false
temporary_directory=
atomic_path=

fail() {
  printf 'blobyard installer: %s\n' "$1" >&2
  exit 1
}

usage() {
  printf '%s\n' 'Usage: install.sh [--version <version>] [--install-dir <directory>] [--dry-run]'
}

require_tool() {
  command -v "$1" >/dev/null 2>&1 || fail "required tool is unavailable: $1"
}

cleanup() {
  if [ -n "$temporary_directory" ] && [ -d "$temporary_directory" ]; then
    rm -rf "$temporary_directory"
  fi
  if [ -n "$atomic_path" ] && [ -f "$atomic_path" ]; then
    rm -f "$atomic_path"
  fi
}

safe_asset_name() {
  case $1 in
    '' | */* | *\\* | .*) return 1 ;;
    *[!A-Za-z0-9._-]*) return 1 ;;
    *) return 0 ;;
  esac
}

platform_key() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$os:$arch" in
    Darwin:arm64) printf '%s\n' darwin-arm64 ;;
    Darwin:x86_64) printf '%s\n' darwin-amd64 ;;
    Linux:aarch64 | Linux:arm64) printf '%s\n' linux-arm64 ;;
    Linux:x86_64 | Linux:amd64) printf '%s\n' linux-amd64 ;;
    *) fail "unsupported platform: $os/$arch" ;;
  esac
}

download() {
  asset=$1
  destination=$2
  safe_asset_name "$asset" || fail "release manifest contains an unsafe asset name"
  case $release_base in
    https://*) set -- --proto '=https' --proto-redir '=https' ;;
    file://*) set -- --proto '=file' --proto-redir '=file' ;;
    *) fail 'release base URL must use HTTPS' ;;
  esac
  curl --fail --location --silent --show-error --connect-timeout 10 --max-time 120 "$@" --output "$destination" "$release_base/$asset"
}

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

verify_checksum_entry() {
  file=$1
  expected=$(awk -v name="$file" '$2 == name { print $1 }' "$checksums_path")
  [ -n "$expected" ] || fail "checksum entry is missing for $file"
  actual=$(sha256_file "$temporary_directory/$file")
  [ "$expected" = "$actual" ] || fail "checksum verification failed for $file"
}

while [ "$#" -gt 0 ]; do
  case $1 in
    --version)
      [ "$#" -ge 2 ] || fail '--version requires a value'
      version=$2
      shift 2
      ;;
    --install-dir)
      [ "$#" -ge 2 ] || fail '--install-dir requires a value'
      install_dir=$2
      shift 2
      ;;
    --dry-run)
      dry_run=true
      shift
      ;;
    --help | -h)
      usage
      exit 0
      ;;
    *) fail "unknown option: $1" ;;
  esac
done

[ -n "$install_dir" ] || fail 'HOME is unset; pass --install-dir explicitly'
platform=$(platform_key)

if [ "$dry_run" = true ]; then
  printf 'Would install Blobyard version %s for %s to %s/blobyard\n' "$version" "$platform" "$install_dir"
  printf '%s\n' 'The install will verify the checksum signature, GitHub provenance, and archive checksum before placement.'
  exit 0
fi

for tool in curl jq gh cosign tar; do
  require_tool "$tool"
done
case $platform in
  darwin-*)
    require_tool codesign
    ;;
esac

if [ "$version" = latest ]; then
  version=$(curl --fail --location --silent --show-error --connect-timeout 10 --max-time 30 \
    --proto '=https' --proto-redir '=https' \
    "$public_release_origin/latest.json" | jq -er '.version')
fi
printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$' || fail "invalid release version: $version"

release_base=${BLOBYARD_RELEASE_BASE_URL:-"$public_release_origin/v$version"}
temporary_directory=$(mktemp -d)
trap cleanup EXIT HUP INT TERM
manifest_path="$temporary_directory/$release_manifest_name"
download "$release_manifest_name" "$manifest_path"

manifest_repository=$(jq -er '.repository' "$manifest_path")
manifest_version=$(jq -er '.version' "$manifest_path")
manifest_workflow=$(jq -er '.signing.workflow' "$manifest_path")
manifest_issuer=$(jq -er '.signing.oidcIssuer' "$manifest_path")
manifest_signing_ref=$(jq -r '.signing.ref // empty' "$manifest_path")
source_revision=$(jq -r '.sourceRevision // empty' "$manifest_path")
[ "$manifest_repository" = "$repository" ] || fail 'release manifest repository is not Blobyard'
[ "$manifest_version" = "$version" ] || fail 'release manifest version does not match the requested release'
if [ "$manifest_workflow" != "$signing_workflow" ] || [ "$manifest_issuer" != "$signing_issuer" ]; then
  fail 'release manifest signing identity is not trusted'
fi
if [ -n "$manifest_signing_ref" ] || [ -n "$source_revision" ]; then
  [ "$manifest_signing_ref" = "$trusted_signing_ref" ] || fail 'release manifest source identity is not trusted'
  printf '%s\n' "$source_revision" | grep -Eq '^[0-9a-f]{40}$' || fail 'release manifest source identity is not trusted'
else
  manifest_signing_ref="refs/tags/v$version"
fi

checksums=$(jq -er '.assets.checksums' "$manifest_path")
signature=$(jq -er '.assets.checksumsSignature' "$manifest_path")
provenance=$(jq -er '.assets.provenance' "$manifest_path")
archive=$(jq -er --arg key "$platform" '.platforms[] | select(.key == $key) | .archive' "$manifest_path")
executable=$(jq -er --arg key "$platform" '.platforms[] | select(.key == $key) | .executable' "$manifest_path")
[ -n "$archive" ] || fail "release has no asset for $platform"
safe_asset_name "$executable" || fail 'release manifest contains an unsafe executable name'

checksums_path="$temporary_directory/$checksums"
signature_path="$temporary_directory/$signature"
provenance_path="$temporary_directory/$provenance"
download "$checksums" "$checksums_path"
download "$signature" "$signature_path"
download "$provenance" "$provenance_path"
identity="https://github.com/$repository/$signing_workflow@$manifest_signing_ref"
if [ -n "$source_revision" ]; then
  cosign verify-blob \
    --bundle "$signature_path" \
    --certificate-identity "$identity" \
    --certificate-oidc-issuer "$signing_issuer" \
    --certificate-github-workflow-sha "$source_revision" \
    "$checksums_path" >/dev/null || fail 'checksum signature verification failed'
else
  cosign verify-blob \
    --bundle "$signature_path" \
    --certificate-identity "$identity" \
    --certificate-oidc-issuer "$signing_issuer" \
    "$checksums_path" >/dev/null || fail 'checksum signature verification failed'
fi

verify_provenance() {
  provenance_target=$1
  if [ -n "$source_revision" ]; then
    gh attestation verify "$provenance_target" \
      --repo "$repository" \
      --bundle "$provenance_path" \
      --cert-identity "$identity" \
      --source-ref "$manifest_signing_ref" \
      --source-digest "$source_revision" \
      --signer-digest "$source_revision" >/dev/null
  else
    gh attestation verify "$provenance_target" \
      --repo "$repository" \
      --bundle "$provenance_path" \
      --cert-identity "$identity" \
      --source-ref "$manifest_signing_ref" >/dev/null
  fi
}

verify_provenance "$checksums_path" || fail 'checksum provenance verification failed'
verify_checksum_entry "$release_manifest_name"

download "$archive" "$temporary_directory/$archive"
verify_checksum_entry "$archive"
verify_provenance "$temporary_directory/$archive" || fail 'artifact provenance verification failed'

members=$(tar -tzf "$temporary_directory/$archive")
[ "$members" = "$executable" ] || fail 'release archive contains unexpected paths'
tar -xzf "$temporary_directory/$archive" -C "$temporary_directory" "$executable"
if [ ! -f "$temporary_directory/$executable" ] || [ -L "$temporary_directory/$executable" ]; then
  fail 'release executable is not a regular file'
fi
case $platform in
  darwin-*)
    codesign --verify --deep --strict "$temporary_directory/$executable" || fail 'Apple code-signature verification failed'
    ;;
esac
mkdir -p "$install_dir"
atomic_path=$(mktemp "$install_dir/.blobyard.XXXXXX")
install -m 0755 "$temporary_directory/$executable" "$atomic_path"
mv -f "$atomic_path" "$install_dir/blobyard"
atomic_path=
printf 'Installed Blobyard %s to %s/blobyard\n' "$version" "$install_dir"
