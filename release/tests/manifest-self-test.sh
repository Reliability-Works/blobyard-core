#!/usr/bin/env bash

set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
# shellcheck source=release/tests/verifier-fixtures.sh
source "$root/release/tests/verifier-fixtures.sh"
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
version=1.2.3
source_revision=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
manifest="$temporary/blobyard-release-manifest.json"
tools="$temporary/tools"
mkdir -p "$tools"
cli_dockerfile="$root/deploy/docker/cli.Dockerfile"
server_release_dockerfile="$root/deploy/docker/server-release.Dockerfile"
server_binary_pattern="bin/\${TARGETARCH}/blobyard-server"
grep -Fq 'SHELL ["/busybox/sh", "-c"]' "$cli_dockerfile"
grep -Fq 'gcr.io/distroless/cc-debian13:nonroot@sha256:' "$cli_dockerfile"
grep -Fq 'SHELL ["/busybox/sh", "-c"]' "$server_release_dockerfile"
grep -Fq "$server_binary_pattern" "$server_release_dockerfile"
grep -Fq 'gcr.io/distroless/cc-debian13:nonroot@sha256:' "$server_release_dockerfile"
if grep -Fq 'cargo build' "$server_release_dockerfile"; then
  printf 'Release server image recompiles an already verified binary.\n' >&2
  exit 1
fi
if grep -Fq 'gcr.io/distroless/cc-debian12:' "$cli_dockerfile"; then
  printf 'Docker runtime uses a glibc version older than the release builders.\n' >&2
  exit 1
fi

"$root/release/generate-release-manifest.sh" "$version" "$source_revision" "$manifest"
if "$root/release/generate-release-manifest.sh" "$version" deadbeef "$temporary/invalid.json" \
  >/dev/null 2>&1; then
  printf 'Release manifest accepted a short source revision.\n' >&2
  exit 1
fi
expected_targets=(
  aarch64-apple-darwin
  x86_64-apple-darwin
  aarch64-unknown-linux-gnu
  x86_64-unknown-linux-gnu
)
expected_server_targets=(
  aarch64-unknown-linux-gnu
  x86_64-unknown-linux-gnu
)
mapfile -t actual_targets < <(jq -r '.platforms[].target' "$manifest")
[[ ${actual_targets[*]} == "${expected_targets[*]}" ]]
while IFS=$'\t' read -r target archive; do
  [[ $archive == "blobyard-$version-$target.tar.gz" ]]
done < <(jq -r '.platforms[] | [.target, .archive] | @tsv' "$manifest")
mapfile -t actual_server_targets < <(jq -r '.serverPlatforms[].target' "$manifest")
[[ ${actual_server_targets[*]} == "${expected_server_targets[*]}" ]]
while IFS=$'\t' read -r target archive; do
  [[ $archive == "blobyard-server-$version-$target.tar.gz" ]]
done < <(jq -r '.serverPlatforms[] | [.target, .archive] | @tsv' "$manifest")
jq -e '.signing == {
  oidcIssuer: "https://token.actions.githubusercontent.com",
  workflow: ".github/workflows/release.yml",
  ref: "refs/heads/main"
} and .sourceRevision == "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"' "$manifest" >/dev/null

cat >"$temporary/blobyard" <<SH
#!/usr/bin/env sh
printf 'blobyard $version\n'
SH
chmod 0755 "$temporary/blobyard"
for target in "${expected_targets[@]}"; do
  "$root/release/package-artifact.sh" "$version" "$target" "$temporary/blobyard" "$temporary" >/dev/null
done
for target in "${expected_server_targets[@]}"; do
  "$root/release/package-server-artifact.sh" "$version" "$target" "$temporary/blobyard" "$temporary" >/dev/null
done
printf '{"spdxVersion":"SPDX-2.3"}\n' >"$temporary/blobyard-$version.spdx.json"
printf 'packed Action\n' >"$temporary/blobyard-action-$version.tar.gz"
printf 'packed conformance\n' >"$temporary/blobyard-conformance-$version.tar.gz"
printf 'packed SDK\n' >"$temporary/blobyard-sdk-$version.tgz"
metadata="$temporary/container-metadata"
mkdir "$metadata"
jq -n \
  --arg image ghcr.io/reliability-works/blobyard-core-cli \
  --arg digest "sha256:$(printf 'a%.0s' {1..64})" \
  '{image: $image, digest: $digest}' >"$metadata/cli.json"
jq -n \
  --arg image ghcr.io/reliability-works/blobyard-core-server \
  --arg digest "sha256:$(printf 'b%.0s' {1..64})" \
  '{image: $image, digest: $digest}' >"$metadata/server.json"
"$root/release/generate-container-manifest.sh" \
  "$version" "$source_revision" "$metadata" "$temporary/blobyard-container-images.json"

"$root/release/generate-homebrew-formula.sh" "$manifest" "$temporary" "$temporary/blobyard.rb"
ruby -c "$temporary/blobyard.rb" >/dev/null
for target in "${expected_targets[@]}"; do
  grep -Fq "blobyard-$version-$target.tar.gz" "$temporary/blobyard.rb"
done
grep -Fq "https://releases.blobyard.com/v$version" "$temporary/blobyard.rb"
git init -q "$temporary/tap"
"$root/release/update-homebrew-tap.sh" "$temporary/blobyard.rb" "$temporary/tap" >/dev/null
cmp "$temporary/blobyard.rb" "$temporary/tap/Formula/blobyard.rb"

"$root/release/create-checksums.sh" "$manifest" "$temporary"
expected_count=$(jq '
  [.platforms[].archive] +
  [.serverPlatforms[].archive] +
  [
    .releaseManifest,
    .assets.actionBundle,
    .assets.conformanceBundle,
    .assets.containerImages,
    .assets.sbom,
    .assets.sdkPackage,
    .assets.homebrewFormula
  ] | length
' "$manifest")
actual_count=$(wc -l <"$temporary/SHA256SUMS" | tr -d ' ')
[[ $actual_count == "$expected_count" ]]
printf 'keyless checksum signature bundle\n' >"$temporary/SHA256SUMS.sig"
printf 'GitHub provenance bundle\n' >"$temporary/blobyard-provenance.intoto.jsonl"

write_fake_release_verifiers "$tools"
export BLOBYARD_VERIFY_LOG="$temporary/verify.log"
export FAKE_EXPECT_IDENTITY="https://github.com/Reliability-Works/blobyard-core/.github/workflows/release.yml@refs/heads/main"
export FAKE_EXPECT_REF=refs/heads/main
export FAKE_EXPECT_SHA=$source_revision
PATH="$tools:$PATH" "$root/release/verify-release.sh" "$temporary" >/dev/null
[[ $(head -n 1 "$BLOBYARD_VERIFY_LOG") == cosign:SHA256SUMS ]]
grep -Fq 'gh:SHA256SUMS' "$BLOBYARD_VERIFY_LOG"
PATH="$tools:$PATH" "$root/release/verify-containers.sh" "$temporary" >/dev/null
[[ $(grep -c '^cosign-image:' "$BLOBYARD_VERIFY_LOG") == 2 ]]
[[ $(grep -c '^gh-image:' "$BLOBYARD_VERIFY_LOG") == 2 ]]
PATH="$tools:$PATH" /bin/bash "$root/release/verify-release.sh" "$temporary" >/dev/null

legacy="$temporary/legacy"
mkdir "$legacy"
cp "$temporary"/*.tar.gz "$temporary"/*.tgz "$temporary"/*.json "$temporary"/*.rb \
  "$temporary"/*.spdx.json "$legacy"
jq 'del(.sourceRevision, .signing.ref)' "$manifest" >"$legacy/blobyard-release-manifest.json"
"$root/release/create-checksums.sh" "$legacy/blobyard-release-manifest.json" "$legacy"
printf 'keyless checksum signature bundle\n' >"$legacy/SHA256SUMS.sig"
printf 'GitHub provenance bundle\n' >"$legacy/blobyard-provenance.intoto.jsonl"
export FAKE_EXPECT_IDENTITY="https://github.com/Reliability-Works/blobyard-core/.github/workflows/release.yml@refs/tags/v$version"
export FAKE_EXPECT_REF="refs/tags/v$version"
unset FAKE_EXPECT_SHA
PATH="$tools:$PATH" "$root/release/verify-release.sh" "$legacy" >/dev/null

docker_context="$temporary/docker"
"$root/release/prepare-docker-context.sh" "$temporary" "$docker_context"
(cd "$docker_context" && sha256sum --check --strict SHA256SUMS >/dev/null)
[[ $("$docker_context/bin/amd64/blobyard") == "blobyard $version" ]]

server_docker_context="$temporary/server-docker"
"$root/release/prepare-docker-context.sh" "$temporary" "$server_docker_context" server
(cd "$server_docker_context" && sha256sum --check --strict SHA256SUMS >/dev/null)
[[ $("$server_docker_context/bin/amd64/blobyard-server") == "blobyard $version" ]]

if "$root/release/prepare-docker-context.sh" "$temporary" "$temporary/invalid-docker" invalid \
  >/dev/null 2>&1; then
  printf 'Docker context accepted an unsupported release surface.\n' >&2
  exit 1
fi

linux_archive=$(jq -er '.platforms[] | select(.key == "linux-amd64") | .archive' "$manifest")
printf 'tamper\n' >>"$temporary/$linux_archive"
if PATH="$tools:$PATH" "$root/release/verify-release.sh" "$temporary" >/dev/null 2>&1; then
  printf 'Tampered release archive passed verification.\n' >&2
  exit 1
fi

first="$temporary/first"
second="$temporary/second"
mkdir "$first" "$second"
"$root/release/package-artifact.sh" "$version" aarch64-apple-darwin "$temporary/blobyard" "$first" >/dev/null
"$root/release/package-artifact.sh" "$version" aarch64-apple-darwin "$temporary/blobyard" "$second" >/dev/null
cmp "$first/blobyard-$version-aarch64-apple-darwin.tar.gz" "$second/blobyard-$version-aarch64-apple-darwin.tar.gz"

printf 'release manifest self-test passed\n'
