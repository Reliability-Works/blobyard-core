# Blob Yard Core

Blob Yard Core is the self-hostable Blob Yard runtime and the canonical home of the `blobyard` CLI,
MCP server, API client, GitHub Action, storage adapters, OpenAPI contract, conformance bundle, and
release tooling. It is public source under Apache License 2.0.

## A letter to the agent working here

You are working on the part of Blob Yard that anyone can run. The goal is not a demo server or a
thin wrapper around a bucket. The goal is a dependable file layer that one developer can operate on
a single machine, and that a developer, a CI job, or an agent can drive from the CLI, the API, MCP,
or a GitHub Action without learning cloud IAM first.

Treat agents as first-class users. A capable agent should be able to discover a stable contract,
authenticate with narrowly scoped authority, complete the same workflows as a person, and inspect
what happened. Prefer CLI, API, and MCP control over interfaces that only exist for people.

Fight for the obvious behavior. Files are durable until the owner deletes them or a retention policy
removes them. A share can expire without deleting its file. An upload inbox grants a bounded way to
receive data without granting access to everything else. If implementation details make those truths
hard to explain, simplify the implementation or improve the contract instead of teaching users
internal machinery.

Security is part of that obviousness. Users, CLIs, CI jobs, and agents never receive permanent
storage credentials. Capabilities are scoped, short-lived, revocable, and auditable. Raw tokens are
returned once, stored only as hashes, and never appear in logs, screenshots, tests, list APIs, or
command output.

## The working agreement

Read [`VISION.md`](VISION.md) before product work. It defines what Blob Yard Core aims to be, which
features and issues belong here, and how changes are reviewed. When a change conflicts with
`VISION.md`, raise the conflict instead of shipping it.

Preserve user-created and unrelated changes. Never clean, reset, restore, or rewrite work you do not
own. Do not use destructive Git commands. Do not commit, push, tag, publish, or create releases
without explicit authorization.

Keep the boundary. This repository is the self-hostable core under Apache License 2.0. Blob Yard
Cloud's web application, Convex functions, edge proxy, identity, billing, email, and production
infrastructure belong in the private `Reliability-Works/blobyard` repository, not here. Nothing in
this repository may require a Blob Yard Cloud account to operate.

Keep business logic in the Rust domain and service crates. Adapters stay thin. The default operator
path is a single Rust server with SQLite and filesystem storage; S3-compatible storage is optional.
Use pnpm for JavaScript and TypeScript dependencies. Do not use npm, npx, Yarn, or Bun.

Keep contract, implementation, tests, documentation, and conformance evidence aligned. When the
contract changes, the OpenAPI documents, the conformance bundle, and the operator documentation
change in the same piece of work.

## What good work looks like

Make the narrowest correct change, then prove the behavior at the layer that owns it. All limits
enforced by the repository gates are hard failures. Do not weaken coverage, duplication, lint,
complexity, file-size, function-size, audit, or secret gates, and do not broaden exclusions or add
blanket suppressions. Rust denies unsafe code, Clippy warnings, production `unwrap` or `expect`, and
panic-based normal control flow.

Before handing over implementation work, run:

```bash
./scripts/check.sh all
```

Report exact commands and results. Build success does not prove an operator journey, so run the
filesystem and MinIO acceptance paths when runtime behavior changes. When you finish, leave the
repository more truthful than you found it: the contract, implementation, tests, documentation, and
release artifacts should all describe the same product.
