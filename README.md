# Blob Yard Core

Blob Yard Core is the self-hostable file layer for developers, CI, and agents. It stores build
artifacts and generated files durably, keeps them private by default, and exposes controlled upload,
download, sharing, preview, automation, and Web Yard workflows without giving users permanent
storage credentials.

This repository is currently private while extraction and release validation are completed. Public
visibility and public artifacts require a separate approval.

## What is included

- The standalone Rust `blobyard` server
- SQLite metadata storage
- Filesystem and S3-compatible object storage adapters
- The canonical `blobyard` CLI
- The Blob Yard MCP server
- The typed API client and TypeScript SDK
- The GitHub upload Action
- OpenAPI contracts and conformance fixtures
- Backup, restore, reconciliation, and hosted-to-core migration tooling

Blob Yard Cloud remains a separate proprietary product. Its Next.js application, Convex functions,
edge proxy, identity, billing, email, and production operations are not part of this repository.

## Quick start

The supported operator journey is documented in
[`docs/self-hosting/quickstart.md`](docs/self-hosting/quickstart.md). The default Compose path uses
SQLite and filesystem storage. A MinIO-backed S3-compatible path is also provided.

## Repository checks

Install the pinned local dependencies and hooks:

```bash
./scripts/bootstrap.sh
```

Run every release-blocking gate:

```bash
./scripts/check.sh all
```

The complete gate enforces deterministic formatting, package-manager policy, workflow linting,
strict Rust linting, TypeScript checks, exact 100 percent Rust coverage, zero detected duplication,
secret scanning, dependency policy, release builds, contract generation, operator acceptance, and
negative controls for the gates themselves.

## Architecture and security

- [Architecture](ARCHITECTURE.md)
- [Security policy](SECURITY.md)
- [Release candidate contract](docs/release.md)
- [Contributing](CONTRIBUTING.md)

Blob Yard Core is licensed under the Apache License 2.0.
