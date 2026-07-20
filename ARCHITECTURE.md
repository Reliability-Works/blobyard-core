# Architecture

Blob Yard Core is a single-node Rust service with explicit repository and object-storage ports.

## Runtime

`blobyard-server` owns HTTP presentation, authorization, capability issuance, retention planning,
audit records, metadata transactions, and storage orchestration. Route handlers translate requests
and responses but do not own business rules.

`blobyard-repository-sqlite` owns durable metadata and forward-only schema migrations. SQLite runs
in WAL mode on a durable volume.

`blobyard-storage-filesystem` is the default object adapter. `blobyard-storage-s3` supports
S3-compatible providers without exposing provider credentials to clients.

## User surfaces

`blobyard-cli` is the canonical standalone binary. `blobyard-mcp` exposes the same bounded
operations to agents. `blobyard-api-client` and `sdk/typescript` bind the typed HTTP contract. The
GitHub Action installs a verified CLI artifact and delegates transfers to it.

## Contracts

The YAML sources in `openapi/` define shared Core operations and hosted extensions. The generated
`conformance/` bundle pins operation ownership, authorization vectors, behavior fixtures, and
checksums. Generation must be deterministic and the committed bundle must remain current.

## Recovery and migration

Backup, restore, reconciliation, and hosted-to-core migration verify object versions and checksums.
Migration imports into an empty standalone installation and fails closed on identity, metadata, or
byte mismatches.

## Release trust

Release workflows build native binaries from an exact tag, sign and notarize macOS artifacts,
generate checksums and SBOMs, and attach GitHub provenance. The installer verifies repository,
workflow identity, source revision, signature, checksums, archive shape, executable mode, and binary
version before atomic placement.
