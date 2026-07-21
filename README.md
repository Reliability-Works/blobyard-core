# Blob Yard Core

Blob Yard Core is the self-hostable file layer for developers, CI, and agents. It stores build
artifacts and generated files durably, keeps them private by default, and exposes controlled upload,
download, sharing, preview, automation, and Web Yard workflows without giving users permanent
storage credentials.

One Rust server, one SQLite database, one storage directory or S3-compatible bucket. No cloud IAM
homework: users, CLIs, CI jobs, and agents receive scoped, short-lived, revocable capabilities
instead of storage credentials.

The source is published under Apache License 2.0. Signed release artifacts are produced only from
the repository's verified release workflow.

## Contents

- [What is included](#what-is-included)
- [Install the CLI](#install-the-cli)
- [Verify the release](#verify-the-release)
- [Use Blob Yard from an agent or CI](#use-blob-yard-from-an-agent-or-ci)
- [Self-host the server](#self-host-the-server)
- [Repository checks](#repository-checks)
- [Documentation](#documentation)
- [License](#license)

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

## Install the CLI

macOS and Linux, on Apple Silicon and x86-64:

```bash
curl --proto '=https' --tlsv1.2 --fail --silent --show-error \
  https://raw.githubusercontent.com/Reliability-Works/blobyard-core/main/scripts/install.sh | sh
```

The installer places `blobyard` in `~/.local/bin`. Pin an exact release or change the destination
when you need to:

```bash
curl --proto '=https' --tlsv1.2 --fail --silent --show-error \
  https://raw.githubusercontent.com/Reliability-Works/blobyard-core/main/scripts/install.sh \
  | sh -s -- --version 0.1.12 --install-dir /usr/local/bin
```

Confirm the installation:

```bash
blobyard --version
blobyard --help
```

Then log in against Blob Yard Cloud or your own server and make your first upload:

```bash
blobyard login
blobyard upload ./dist/app.zip --path releases/app.zip
```

The full command reference lives in [`docs/cli.md`](docs/cli.md).

## Verify the release

Every release asset is checksummed, signed with a keyless Sigstore signature, and published with
GitHub provenance from the release workflow on `main`. The installer verifies all three before it
writes anything. To verify a download yourself, follow [`docs/release.md`](docs/release.md).

## Use Blob Yard from an agent or CI

Agents and CI jobs are first-class users. They authenticate with narrowly scoped authority and
complete the same workflows as a person.

**MCP.** Point any MCP-capable agent at the stdio server. It reuses the CLI's approved session:

```bash
blobyard mcp serve --stdio
```

Configuration details and the tool catalog are in [`docs/cli.md`](docs/cli.md).

**GitHub Actions.** The composite Action installs a verified CLI release and obtains a short-lived
machine identity through GitHub OIDC, so no stored secret is required:

```yaml
permissions:
  contents: read
  id-token: write

steps:
  - uses: actions/checkout@v7
  - uses: Reliability-Works/blobyard-core/.github/actions/upload@v1
    with:
      api-url: https://api.blobyard.com/v1
      path: ./dist/app.zip
      project: mobile
      share: true
      expires: 7d
```

Upload, share, PR comment, and Web Yard deploy inputs are documented in
[`docs/action.md`](docs/action.md).

**Non-interactive install.** Automation can run the installer unchanged; it is non-interactive,
verifies the release before installing, and accepts `--version` and `--install-dir` for hermetic
setups.

**API and SDK.** Every operation is available over the typed `/v1` API and the TypeScript SDK under
[`sdk/`](sdk/). Contracts are in [`openapi/`](openapi/).

## Self-host the server

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

## Documentation

- [Vision and scope](VISION.md): what Blob Yard Core aims to be, what belongs here, and how changes
  are reviewed
- [Working with agents](AGENTS.md): the working agreement for agent contributors
- [Architecture](ARCHITECTURE.md)
- [Security policy](SECURITY.md)
- [Release candidate contract](docs/release.md)
- [Contributing](CONTRIBUTING.md)

## License

Blob Yard Core is licensed under the Apache License 2.0. See [LICENSE](LICENSE) and
[NOTICE](NOTICE). Contributions are accepted under the same license with a Developer Certificate
of Origin sign-off, as described in [CONTRIBUTING.md](CONTRIBUTING.md).

The Blob Yard name, the `blobyard` mark, and the Blob Yard logo are trademarks of
Reliability Works Ltd. The Apache License 2.0 does not grant trademark rights (section 6). You may
build and distribute forks of this code, but do not name or brand a fork, hosted service, or
derived product in a way that suggests it is Blob Yard or is endorsed by Reliability Works Ltd.
