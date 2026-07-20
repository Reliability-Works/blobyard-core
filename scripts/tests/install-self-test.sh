#!/usr/bin/env bash

set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
# shellcheck source=release/tests/verifier-fixtures.sh
source "$root/release/tests/verifier-fixtures.sh"
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
release_directory="$temporary/release"
tools="$temporary/tools"
install_directory="$temporary/install"
mkdir -p "$release_directory" "$tools" "$install_directory"
version=1.2.3
source_revision=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
grep -Fq 'public_release_origin=https://releases.blobyard.com' "$root/scripts/install.sh"

cat >"$tools/uname" <<'SH'
#!/usr/bin/env sh
case ${1:-} in
  -s) printf '%s\n' "${FAKE_UNAME_OS:?}" ;;
  -m) printf '%s\n' "${FAKE_UNAME_ARCH:?}" ;;
  *) exit 2 ;;
esac
SH
chmod 0755 "$tools/uname"
write_fake_release_verifiers "$tools"
cat >"$tools/codesign" <<'SH'
#!/usr/bin/env sh
[ "${FAKE_APPLE_FAIL:-0}" = 0 ]
SH
chmod 0755 "$tools/codesign"

cat >"$temporary/blobyard" <<'SH'
#!/usr/bin/env sh
printf 'blobyard fixture\n'
SH
chmod 0755 "$temporary/blobyard"
archive="blobyard-$version-aarch64-apple-darwin.tar.gz"
COPYFILE_DISABLE=1 tar -czf "$release_directory/$archive" -C "$temporary" blobyard
linux_archive="blobyard-$version-x86_64-unknown-linux-gnu.tar.gz"
cp "$release_directory/$archive" "$release_directory/$linux_archive"
"$root/release/generate-release-manifest.sh" "$version" "$source_revision" "$release_directory/blobyard-release-manifest.json"
printf 'GitHub provenance bundle\n' >"$release_directory/blobyard-provenance.intoto.jsonl"
printf 'keyless checksum signature bundle\n' >"$release_directory/SHA256SUMS.sig"

digest() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

{
  printf '%s  %s\n' "$(digest "$release_directory/blobyard-release-manifest.json")" blobyard-release-manifest.json
  printf '%s  %s\n' "$(digest "$release_directory/$archive")" "$archive"
  printf '%s  %s\n' "$(digest "$release_directory/$linux_archive")" "$linux_archive"
} >"$release_directory/SHA256SUMS"

while read -r fake_os fake_arch expected_platform; do
  output=$(FAKE_UNAME_OS=$fake_os FAKE_UNAME_ARCH=$fake_arch PATH="$tools:$PATH" "$root/scripts/install.sh" --dry-run --version "$version")
  grep -Fq "$expected_platform" <<<"$output"
done <<'PLATFORMS'
Darwin arm64 darwin-arm64
Darwin x86_64 darwin-amd64
Linux aarch64 linux-arm64
Linux x86_64 linux-amd64
PLATFORMS

FAKE_UNAME_OS=Darwin FAKE_UNAME_ARCH=arm64 PATH="$tools:$PATH" \
  BLOBYARD_RELEASE_BASE_URL="file://$release_directory" \
  BLOBYARD_VERIFY_LOG="$temporary/verify.log" \
  FAKE_EXPECT_IDENTITY="https://github.com/Reliability-Works/blobyard-core/.github/workflows/release.yml@refs/heads/main" \
  FAKE_EXPECT_REF=refs/heads/main \
  FAKE_EXPECT_SHA=$source_revision \
  "$root/scripts/install.sh" --version "$version" --install-dir "$install_directory"
[[ $("$install_directory/blobyard") == 'blobyard fixture' ]]
[[ $(head -n 1 "$temporary/verify.log") == cosign:SHA256SUMS ]]
[[ $(sed -n '2p' "$temporary/verify.log") == gh:SHA256SUMS ]]

legacy_release_directory="$temporary/legacy-release"
legacy_install_directory="$temporary/legacy-install"
cp -R "$release_directory" "$legacy_release_directory"
mkdir "$legacy_install_directory"
jq 'del(.sourceRevision, .signing.ref)' \
  "$release_directory/blobyard-release-manifest.json" \
  >"$legacy_release_directory/blobyard-release-manifest.json"
{
  printf '%s  %s\n' "$(digest "$legacy_release_directory/blobyard-release-manifest.json")" blobyard-release-manifest.json
  printf '%s  %s\n' "$(digest "$legacy_release_directory/$archive")" "$archive"
  printf '%s  %s\n' "$(digest "$legacy_release_directory/$linux_archive")" "$linux_archive"
} >"$legacy_release_directory/SHA256SUMS"
FAKE_UNAME_OS=Darwin FAKE_UNAME_ARCH=arm64 PATH="$tools:$PATH" \
  BLOBYARD_RELEASE_BASE_URL="file://$legacy_release_directory" \
  BLOBYARD_VERIFY_LOG="$temporary/legacy-verify.log" \
  FAKE_EXPECT_IDENTITY="https://github.com/Reliability-Works/blobyard-core/.github/workflows/release.yml@refs/tags/v$version" \
  FAKE_EXPECT_REF="refs/tags/v$version" \
  "$root/scripts/install.sh" --version "$version" --install-dir "$legacy_install_directory"
[[ $("$legacy_install_directory/blobyard") == 'blobyard fixture' ]]

linux_install_directory="$temporary/linux-install"
FAKE_UNAME_OS=Linux FAKE_UNAME_ARCH=x86_64 PATH="$tools:$PATH" \
  BLOBYARD_RELEASE_BASE_URL="file://$release_directory" \
  BLOBYARD_VERIFY_LOG="$temporary/linux-verify.log" \
  "$root/scripts/install.sh" --version "$version" --install-dir "$linux_install_directory"
[[ $("$linux_install_directory/blobyard") == 'blobyard fixture' ]]

cp "$install_directory/blobyard" "$temporary/installed-before-verifier-failure"
if FAKE_UNAME_OS=Darwin FAKE_UNAME_ARCH=arm64 PATH="$tools:$PATH" \
  BLOBYARD_RELEASE_BASE_URL="file://$release_directory" \
  BLOBYARD_VERIFY_LOG="$temporary/signature-failure.log" FAKE_COSIGN_FAIL=1 \
  "$root/scripts/install.sh" --version "$version" --install-dir "$install_directory" 2>"$temporary/signature-error.log"; then
  printf 'invalid checksum signature was accepted\n' >&2
  exit 1
fi
grep -Fq 'checksum signature verification failed' "$temporary/signature-error.log"
cmp "$temporary/installed-before-verifier-failure" "$install_directory/blobyard"

if FAKE_UNAME_OS=Darwin FAKE_UNAME_ARCH=arm64 PATH="$tools:$PATH" \
  BLOBYARD_RELEASE_BASE_URL="file://$release_directory" \
  BLOBYARD_VERIFY_LOG="$temporary/provenance-failure.log" FAKE_GH_FAIL=1 \
  "$root/scripts/install.sh" --version "$version" --install-dir "$install_directory" 2>"$temporary/provenance-error.log"; then
  printf 'invalid checksum provenance was accepted\n' >&2
  exit 1
fi
grep -Fq 'checksum provenance verification failed' "$temporary/provenance-error.log"
cmp "$temporary/installed-before-verifier-failure" "$install_directory/blobyard"

if FAKE_UNAME_OS=Darwin FAKE_UNAME_ARCH=arm64 PATH="$tools:$PATH" \
  BLOBYARD_RELEASE_BASE_URL="file://$release_directory" \
  BLOBYARD_VERIFY_LOG="$temporary/apple-failure.log" FAKE_APPLE_FAIL=1 \
  "$root/scripts/install.sh" --version "$version" --install-dir "$install_directory" 2>"$temporary/apple-error.log"; then
  printf 'invalid Apple platform signature was accepted\n' >&2
  exit 1
fi
grep -Fq 'Apple code-signature verification failed' "$temporary/apple-error.log"
cmp "$temporary/installed-before-verifier-failure" "$install_directory/blobyard"

cp "$install_directory/blobyard" "$temporary/installed-before-tamper"
printf 'tamper\n' >>"$release_directory/$archive"
if FAKE_UNAME_OS=Darwin FAKE_UNAME_ARCH=arm64 PATH="$tools:$PATH" \
  BLOBYARD_RELEASE_BASE_URL="file://$release_directory" \
  BLOBYARD_VERIFY_LOG="$temporary/tamper-verify.log" \
  "$root/scripts/install.sh" --version "$version" --install-dir "$install_directory" 2>"$temporary/tamper.log"; then
  printf 'tampered archive was installed\n' >&2
  exit 1
fi
grep -Fq 'checksum verification failed' "$temporary/tamper.log"
cmp "$temporary/installed-before-tamper" "$install_directory/blobyard"
if find "$install_directory" -name '.blobyard.*' -print -quit | grep -q .; then
  printf 'installer left a partial atomic file\n' >&2
  exit 1
fi

if FAKE_UNAME_OS=Plan9 FAKE_UNAME_ARCH=mips PATH="$tools:$PATH" \
  "$root/scripts/install.sh" --dry-run 2>"$temporary/unsupported.log"; then
  printf 'unsupported platform was accepted\n' >&2
  exit 1
fi
grep -Fq 'unsupported platform: Plan9/mips' "$temporary/unsupported.log"

printf 'installer self-test passed\n'
