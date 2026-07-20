# Blob Yard Core

Blob Yard Core is the self-hostable Blob Yard runtime and the canonical home of the `blobyard` CLI,
MCP server, API client, GitHub Action, storage adapters, OpenAPI contract, conformance bundle, and
release tooling.

## Working agreement

- Keep this repository private until Callum Spencer explicitly authorizes publication.
- Do not publish source, packages, releases, images, or artifacts without separate authorization.
- Preserve the Apache 2.0 boundary. Cloud-only web, Convex, edge, identity, billing, email, and
  production infrastructure code belongs in `Reliability-Works/blobyard`, not here.
- Keep business logic in the Rust domain and service crates. Adapters should remain thin.
- The default operator path is a single Rust server with SQLite and filesystem storage.
  S3-compatible storage is optional.
- Raw capabilities and bootstrap tokens are returned once, stored only as hashes, and never logged.
- Use pnpm for JavaScript and TypeScript dependencies. Do not use npm, npx, Yarn, or Bun.
- Do not weaken coverage, duplication, lint, complexity, file-size, function-size, audit, or secret
  gates.
- Keep contract, implementation, tests, documentation, and conformance evidence aligned.

Before handing over implementation work, run:

```bash
./scripts/check.sh all
```

Report exact commands and results. Build success does not prove an operator journey, so run the
filesystem and MinIO acceptance paths when runtime behavior changes.
