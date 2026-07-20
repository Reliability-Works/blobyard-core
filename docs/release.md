# Release candidate contract

Blob Yard Core has one versioned release contract. `release/artifacts.json` is the checked-in source
of truth for the CLI and server targets, packed automation surfaces, container names, checksums,
provenance assets, and trusted GitHub Actions signing identity. MCP ships inside the native
`blobyard` CLI through `blobyard mcp`, not as a second binary.

The repository is public. The release workflow creates and validates a draft candidate, but it
cannot publish the GitHub release, create the public tag, update the public R2 host, update
Homebrew, or make a container public. Those publication actions remain deliberate operator steps.

## Candidate contents

A candidate contains:

- two signed and notarized macOS CLI archives;
- two Linux CLI archives;
- two Linux `blobyard-server` archives;
- an exact composite GitHub Action bundle;
- a source-only TypeScript SDK package whose version matches Core;
- a revision-stamped conformance bundle containing OpenAPI documents, operation metadata,
  authorization vectors, behavior fixtures, and inner checksums;
- one repository SPDX SBOM;
- isolated multi-architecture CLI and server candidate images, pinned by digest in
  `blobyard-container-images.json`;
- a generated Homebrew formula for inspection only;
- the release manifest, exact SHA-256 inventory, keyless checksum signature, GitHub artifact
  attestations, and provenance bundle.

Rust crates and the TypeScript SDK are not published to registries.

## Verification order

Consumers verify the checksum signature against the exact `release.yml` workflow on `main`, the
GitHub OIDC issuer, and the accepted source commit. They then verify GitHub provenance for the
checksum file, require the exact checksum member set, and verify each file before extracting or
executing it.

The Action, SDK, and conformance archives have exact safe file inventories. The conformance bundle
also verifies its own inner checksum file and exact source revision. macOS CLI binaries require a
valid Developer ID signature and successful notarization before packaging.

The CLI and server images are built without a mutable `latest` tag. Candidate packages remain
private until the operator deliberately changes their visibility. The workflow signs each immutable
digest with GitHub OIDC, publishes provenance to the registry, verifies both claims, and runs each
image by digest. Their verified digests enter the signed release file set only after those checks
pass.

No installer, smoke test, hosted acceptance job, or operator path may execute a binary before its
checksum, signature, provenance, archive inventory, source commit, and reported version pass.

## Draft-first workflow

`Build release candidate` is manually dispatched with one full commit on `main` and the exact
prepared semantic version. It verifies that the repository is public and creates or reuses a draft
GitHub release bound to that commit. It then:

1. builds, signs, notarizes, runs, and deterministically packages the four CLI targets;
2. builds, runs, and packages the two Linux standalone server targets;
3. packs and inspects the Action, SDK, and revision-stamped conformance surfaces;
4. generates the release manifest, Homebrew formula, and SPDX SBOM;
5. builds the two isolated multi-architecture candidate images, then signs, attests, verifies, and
   runs their immutable digests;
6. records both digests, generates the final exact checksum set, signs it, and attests every file;
7. downloads the final bundle on every supported native runner, verifies it again, atomically
   installs and runs the CLI, and runs both standalone server archives;
8. attaches the complete verified file bundle to the draft without publishing it.

The local non-publishing checks are:

```bash
release/tests/manifest-self-test.sh
release/tests/surface-assets-self-test.sh
scripts/tests/install-self-test.sh
node scripts/check-workflow-shell.mjs .github/workflows/release.yml
```

Actual Apple signatures, GitHub OIDC signatures, registry attestations, and native platform smoke
tests run only in the protected `release` environment.
