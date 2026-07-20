#!/usr/bin/env bash

write_fake_release_verifiers() {
  local directory=$1
  mkdir -p "$directory"
  cat >"$directory/cosign" <<'SH'
#!/usr/bin/env sh
set -eu
[ "${FAKE_COSIGN_FAIL:-0}" = 0 ] || exit 1
arguments=" $* "
case ${1:-} in
  verify-blob)
    case $arguments in
      *' --bundle '*' --certificate-identity '*' --certificate-oidc-issuer '*) ;;
      *) exit 2 ;;
    esac
    ;;
  verify)
    case $arguments in
      *' --certificate-identity '*' --certificate-oidc-issuer '*' --certificate-github-workflow-sha '*) ;;
      *) exit 2 ;;
    esac
    ;;
  *) exit 2 ;;
esac
if [ -n "${FAKE_EXPECT_IDENTITY:-}" ]; then
  case $arguments in *" --certificate-identity $FAKE_EXPECT_IDENTITY "*) ;; *) exit 2 ;; esac
fi
if [ -n "${FAKE_EXPECT_SHA:-}" ]; then
  case $arguments in *" --certificate-github-workflow-sha $FAKE_EXPECT_SHA "*) ;; *) exit 2 ;; esac
fi
eval "target=\${$#}"
if [ "${1:-}" = verify-blob ]; then
  [ -s "$target" ]
  printf 'cosign:%s\n' "${target##*/}" >>"${BLOBYARD_VERIFY_LOG:?}"
else
  case $target in ghcr.io/*@sha256:*) ;; *) exit 2 ;; esac
  printf 'cosign-image:%s\n' "$target" >>"${BLOBYARD_VERIFY_LOG:?}"
fi
SH
  cat >"$directory/gh" <<'SH'
#!/usr/bin/env sh
set -eu
[ "${1:-}" = attestation ] && [ "${2:-}" = verify ] || exit 2
[ "${FAKE_GH_FAIL:-0}" = 0 ] || exit 1
target=${3:-}
arguments=" $* "
case $target in
  oci://*)
    case $arguments in
      *' --repo '*' --cert-identity '*' --source-ref '*) ;;
      *) exit 2 ;;
    esac
    ;;
  *)
    case $arguments in
      *' --repo '*' --bundle '*' --cert-identity '*' --source-ref '*) ;;
      *) exit 2 ;;
    esac
    ;;
esac
if [ -n "${FAKE_EXPECT_IDENTITY:-}" ]; then
  case $arguments in *" --cert-identity $FAKE_EXPECT_IDENTITY "*) ;; *) exit 2 ;; esac
fi
if [ -n "${FAKE_EXPECT_REF:-}" ]; then
  case $arguments in *" --source-ref $FAKE_EXPECT_REF "*) ;; *) exit 2 ;; esac
fi
if [ -n "${FAKE_EXPECT_SHA:-}" ]; then
  case $arguments in *" --source-digest $FAKE_EXPECT_SHA "*) ;; *) exit 2 ;; esac
  case $arguments in *" --signer-digest $FAKE_EXPECT_SHA "*) ;; *) exit 2 ;; esac
fi
case $target in
  oci://ghcr.io/*@sha256:*) printf 'gh-image:%s\n' "$target" >>"${BLOBYARD_VERIFY_LOG:?}" ;;
  *)
    [ -s "$target" ]
    printf 'gh:%s\n' "${target##*/}" >>"${BLOBYARD_VERIFY_LOG:?}"
    ;;
esac
SH
  chmod 0755 "$directory/cosign" "$directory/gh"
}
