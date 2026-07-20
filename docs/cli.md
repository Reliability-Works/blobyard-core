# Blobyard CLI

The Blobyard CLI is a standalone Rust binary named `blobyard`. It is distributed as signed release
archives, a verified installer, a Docker image, and a Homebrew formula, never as a JavaScript
package.

## Authentication and scope

```bash
blobyard login --name "MacBook Pro"
blobyard whoami
blobyard logout
```

Login opens `https://blobyard.com/cli/activate`, shows an ambiguity-free user code, and waits for
browser approval. Only the rotating refresh token is persisted, preferably in the operating-system
credential manager.

Configuration precedence is command flag, environment, project `.blobyard.toml`, user config, then
the production API. `BLOBYARD_TOKEN` is a temporary scoped CI override and is never written to disk.

The default `cloud` profile keeps the existing production endpoint and credential entry. Add a
self-hosted profile by piping the standalone server's one-time bootstrap token into the CLI:

```bash
blobyard-server bootstrap-token \
  | blobyard profiles add local --api-url http://127.0.0.1:8787 --token-stdin
```

The bootstrap authority is exchanged once for a scoped API token. Only that API token is saved in
the profile's isolated operating system credential entry or isolated `0600` fallback file. The raw
bootstrap token and returned API token are never printed. Select the profile with `--profile`,
`BLOBYARD_PROFILE`, or project config.

```toml
# Written to the platform user config by `profiles add`
[profiles.local]
api_url = "http://127.0.0.1:8787/v1"
web_yard_origin = "http://localhost:8787"
workspace = "default"
project = "demo"
```

```bash
blobyard --profile local whoami
blobyard --profile local init
```

The generated project `.blobyard.toml` records `profile = "local"`, so later commands use the same
endpoint, defaults, and credential slot without repeating the flag. A self-hosted profile never
falls back to the Blob Yard Cloud endpoint when it is missing `api_url`. The bootstrap response also
pins `web_yard_origin`, the trusted domain root whose first-level subdomains serve public Web Yards.
The CLI rejects response URLs outside that origin. Operators may override it explicitly with
`--web-yard-origin` or `BLOBYARD_WEB_YARD_ORIGIN` while testing a new self-hosted routing setup.

The CLI and its MCP server classify every operation from the canonical contract. A Cloud-only
operation selected under a self-hosted profile fails locally with `OPERATION_UNSUPPORTED` before the
client sends a request. Core operations remain identical across both deployment types.

List or create workspaces without opening the dashboard:

```bash
blobyard workspaces list
blobyard workspaces create "Product Team"
blobyard workspaces rename "Platform Team" --workspace product-team
```

## AI agents and MCP

Start the local stdio MCP server from the same signed binary:

```bash
blobyard mcp serve --stdio
```

The server reuses the browser-approved CLI session and typed API client. It writes only MCP protocol
messages to standard output, keeps diagnostics on standard error, and never returns refresh tokens,
R2 credentials, or signed transfer URLs to the agent.

The MCP catalog covers workspace rename and safe account-export operations. Stripe-hosted billing
sessions and both phases of account deletion are intentionally absent because they would return a
hosted payment URL or destructive confirmation capability to model context. Use the authenticated
CLI or API for those operations, and keep final deletion confirmation under direct human control.

## Projects and objects

```bash
blobyard init --workspace acme --project mobile
blobyard projects list
blobyard projects create "Mobile builds"
blobyard upload ./dist --path builds/main
blobyard ls blobyard://acme/mobile/builds --versions
blobyard download blobyard://acme/mobile/builds/app.ipa?version=12 --output ./app.ipa
blobyard rm blobyard://acme/mobile/builds/old.zip
```

A directory upload respects `.gitignore` and `.blobyardignore`, preserves safe relative paths, and
does not follow symlinks. Each re-upload creates an immutable version. An unversioned Blobyard URI
resolves to the current ready version.

## Shares, previews, inboxes, and retention

```bash
blobyard share blobyard://acme/mobile/builds/app.ipa --expires 7d
blobyard shares list
blobyard shares revoke share_123
blobyard preview ./storybook-static --expires 24h
blobyard previews list
blobyard previews revoke preview_123
blobyard inbox create "Tester logs" --expires 24h
blobyard inbox list
blobyard inbox revoke inbox_123
blobyard retention set --latest 20 --branch main --path 'builds/**'
blobyard retention show
```

Raw public URLs are shown once. Subsequent listings contain redacted metadata only. Preview
directories require `index.html` and are served from the isolated preview origin.

## Workspace administration

Dashboard administration also has direct CLI commands. List commands return redacted metadata, and
create commands preserve one-time credential and invitation-link behavior.

```bash
blobyard audit list
blobyard members list
blobyard members role user_123 --role admin
blobyard members remove user_123 --force
blobyard invites create developer@example.com --role member
blobyard invites list
blobyard invites revoke invite_123
blobyard tokens create "Release CI" --expires-days 7 --scope object:write --scope share:manage
blobyard tokens list
blobyard tokens revoke token_123
blobyard trusts list
blobyard sessions list
blobyard sessions revoke session_123
```

`blobyard tokens create` prints the raw token once. MCP intentionally excludes token creation so a
new bearer credential is never returned to model context. Use `blobyard trusts create --help` for
the repository, workflow, ref, environment, and allowed-action fields.

## Billing and account lifecycle

Create hosted Stripe sessions without opening the dashboard first:

```bash
blobyard billing checkout solo
blobyard billing checkout team --seats 5
blobyard billing portal
```

These commands return hosted URLs. Checkout does not purchase a plan until the user completes the
provider flow.

Queue an account export or perform explicit two-phase deletion:

```bash
blobyard account export request
blobyard account delete prepare
blobyard account delete complete <confirmation-token> --force --retry-key <opaque-key>
```

Preparation returns two secrets once: a ten-minute, single-use confirmation and a deletion recovery
capability. It does not delete anything. Completion requires the exact confirmation and `--force`;
without them the CLI stops before making a network request. Cleanup is queued only after the server
rechecks export, membership, and invitation preconditions.

Keep the recovery capability until cleanup reaches `complete`. Deletion revokes ordinary CLI
sessions and API tokens before later cleanup stages, so use the recovery capability as the bearer
only for deletion status, failed-job retry, or an idempotent completion replay:

```bash
read -rs BLOBYARD_TOKEN
export BLOBYARD_TOKEN
blobyard account delete show
blobyard account delete retry --force
```

The server stores only its hash, accepts it for no other API operation, and expires it with the
deletion tombstone. Do not place it in command arguments, logs, or MCP model context.

For billing Checkout or portal creation, account export requests, and account deletion preparation
or completion, choose an opaque retry key before the first attempt and reuse it only for retries of
that exact command:

```bash
blobyard billing checkout team --seats 5 --retry-key checkout-20260715-1
blobyard account delete complete <confirmation-token> --force \
  --retry-key account-delete-20260715-1
```

The CLI derives an endpoint-scoped `Idempotency-Key` from this value. It does not send the raw retry
key or include the confirmation token in the key. A retry with the same command and key replays the
original result; changing the request while reusing the key returns a conflict. Upload reservation
uses its own deterministic content key. Other mutations do not send `--retry-key`, and an ambiguous
result for those operations must be inspected before another attempt. Without `--retry-key`, each
supported CLI invocation remains a new mutation intent.

Human output labels the one-time result as `Share URL: https://blobyard.com/s/<capability>` so it
can be copied directly. `--json` keeps the typed `shareUrl` field, while `--quiet` suppresses the
human line.

## Web Yards

Publish a prebuilt static directory to a stable public destination:

```bash
blobyard --workspace example-team --project web-products \
  deploy ./dist --yard marketing --clean-urls --public
```

The directory must contain `index.html` at its root. The first deployment requires `--public` so a
private file workflow cannot become public by accident. Blob Yard prints the stable
`https://<yard>-<yard-hash>-<workspace>.blobyard.app` URL, the immutable
`https://<yard>-<deployment-hash>-<workspace>.blobyard.app` deployment URL, and the deployment
identifier after the deployment is live. JSON output keeps the stable alias in `yardUrl` and the
exact address in `deploymentUrl`.

Those hosts use the Cloud profile's trusted `https://blobyard.app` origin. A self-hosted profile
uses its own pinned `web_yard_origin` with the same first-level host shape. The CLI accepts only the
exact scheme, port, host label, and domain declared by that profile, with no path, query, fragment,
or lookalike suffix.

A monorepo can define independent destinations in `.blobyard.toml`:

```toml
[yards.marketing]
directory = "apps/marketing/dist"
clean_urls = true

[yards.docs]
directory = "apps/docs/dist"
clean_urls = true

[yards.dashboard]
directory = "apps/dashboard/dist"
spa = true
```

Deploy every configured destination from the directory containing that configuration:

```bash
blobyard deploy --all --public
```

Mixed results do not reverse successful deployments. The command reports every Yard result so the
failed name can be corrected and redeployed independently.

Inspect and manage deployment history:

```bash
blobyard yard list
blobyard yard show marketing
blobyard yard history marketing
blobyard yard rollback marketing <deploy-id>
blobyard yard delete marketing --force
```

See [web-yards.md](web-yards.md) for routing behavior, API and MCP automation, retention, plan
limits, public-content isolation, and recovery guidance.

## Output contract

`--json` emits one JSON document on standard output. Progress and redacted diagnostics use standard
error. `--quiet` and `--verbose` are mutually exclusive.

Stable exit classes include usage `2`, authentication `10`, forbidden `11`, not found `12`, conflict
`13`, plan limit `14`, network `20`, provider `21`, transfer/integrity `22`, rate limit `23`,
internal invariant `70`, and interruption `130`.

The generated [API surface parity inventory](api-surfaces.generated.md) maps every public OpenAPI
operation to its exact CLI command or its explicit exclusion. `pnpm openapi:check` fails when the
generated inventory drifts, while the CLI parity test fails when a declared command path is absent.

Generate shell completion without evaluating downloaded code:

```bash
blobyard completion zsh > ~/.zfunc/_blobyard
```
