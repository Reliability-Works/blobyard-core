# Blobyard HTTP API

The production API is `https://api.blobyard.com/v1`. Development exposes the same versioned contract
through the edge Worker.

The currently deployed machine-readable route and authentication inventory is available at
[`https://blobyard.com/openapi.json`](https://blobyard.com/openapi.json). The checked-in release
candidate contract contains 73 public operations. It remains a candidate until deployment and hosted
acceptance pass, so it must not be assumed to match the hosted contract yet. Internal provider
webhooks and production-acceptance routes are intentionally excluded. Edge-only preview and Web Yard
resolution routes are also not customer operations.

The checked-in OpenAPI document is the canonical operation inventory for this release candidate.
Every operation declares whether it is available through the TypeScript SDK, its explicit native CLI
command path, and its MCP tool. A surface may be excluded only with a reason, for example when
returning a new bearer credential to model context would be unsafe. Regenerate and validate the
derived inventories with:

```bash
pnpm openapi:generate
pnpm openapi:check
```

The checked-in [API surface parity inventory](api-surfaces.generated.md) is generated from that
metadata. The native CLI and MCP presentation layers stay handwritten, while Rust tests prove that
every declared command path and tool exists.

## TypeScript SDK

The dependency-free `@blobyard/sdk` package in `sdk/typescript` derives its operation names,
methods, and paths from OpenAPI. It is workspace-only, marked private, and has not been published to
a package registry:

```ts
import { BlobYardClient } from "@blobyard/sdk";

const client = new BlobYardClient({ accessToken: process.env.BLOBYARD_TOKEN });
const projects = await client.operations.listProjects({
  query: { workspace: "example-team" },
});
```

The contract defines operation-specific query values, JSON bodies, and success data for every
operation. The generator derives those bindings directly from OpenAPI and rejects generic success
schemas, missing standard error responses, and unclassified surface decisions. Deliberately opaque,
redaction-safe audit metadata is the only schema that permits open-ended keys.

## Response envelope

Every response includes `X-Request-Id` and a matching `requestId` field.

```json
{ "ok": true, "data": {}, "requestId": "req_example" }
```

```json
{
  "ok": false,
  "error": { "code": "AUTH_REQUIRED", "message": "Sign in with blobyard login." },
  "requestId": "req_example"
}
```

`Idempotency-Key` is available only for upload reservation, billing Checkout and portal session
creation, account export requests, and account deletion preparation or completion. Those operations
durably replay the original result. Other mutations do not accept a retry key and must not be
retried after an ambiguous result. Rate-limited responses return HTTP 429 and `Retry-After`. Error
bodies never expose stack traces, internal object keys, provider secrets, or resource existence
outside the caller's authority.

## Endpoint groups

| Group               | Routes                                                                                                          |
| ------------------- | --------------------------------------------------------------------------------------------------------------- |
| Readiness           | `GET /v1/health`                                                                                                |
| CLI sessions        | device start/poll, token refresh, logout, whoami                                                                |
| Workspaces/projects | list, create, and bearer-authenticated workspace rename                                                         |
| Objects/transfers   | list/delete, upload request/parts/status/complete/abort, download request                                       |
| Capabilities        | share create/list/resolve/download/revoke, preview create/list/resolve/revoke, inbox create/list/resolve/revoke |
| Retention           | read, replace, and remove project policy                                                                        |
| Web Yards           | start/finalise/fail deploy, list Yards and history, rollback, and delete                                        |
| Automation          | GitHub OIDC exchange                                                                                            |
| Administration      | audit, members, invites, API tokens, CI trusts, and CLI sessions                                                |
| Billing             | hosted paid-plan checkout and billing portal sessions                                                           |
| Account lifecycle   | portable export plus two-phase account deletion                                                                 |
| Public utility      | client-encrypted one-time secret create and redeem                                                              |

For production clients, the hosted OpenAPI document at `https://blobyard.com/openapi.json` is the
authoritative inventory of deployed public methods and paths. The checked-in 73-operation release
candidate adds its intended principal, purpose, and surface decisions, but becomes the hosted
contract only after deployment and acceptance. Convex HTTP routes carry resource identifiers in
validated query parameters or bodies because the router does not expose dynamic path parameters.

## Authentication

- Browser product functions use a Better Auth session through the first-party application origin.
- CLI routes use opaque short-lived access tokens and rotating refresh tokens.
- CI routes use a machine token minted from a configured GitHub OIDC trust.
- Public share and inbox routes use a raw capability token whose hash is stored server-side.
- Preview resolution additionally requires the edge Worker's internal credential.
- Web Yard reads require `yard:read` or `yard:manage`. Deployment, rollback, and deletion require
  `yard:manage`; deployment also transfers bytes with the upload scope.

Every route resolves one principal and then verifies action, role, workspace ownership, resource
ownership, current token state, and plan entitlement.

Account-level billing, export, and deletion routes reject project-scoped API tokens even when the
token otherwise has the requested scope. Billing requires `billing:manage`, export requires
`account:export`, and deletion requires `account:delete`. Workspace rename requires `project:write`
plus access to the named workspace.

Agents may call this API directly with a scoped Blob Yard session, but credentials must stay in the
agent host's secret store rather than prompts, transcripts, or logs. For local agent use, prefer the
MCP server in the signed CLI because it reuses the approved device session without pasted tokens.

`GET /v1/cli/whoami` identifies the caller with `principalType: "cli" | "ci"`, its granted `scopes`,
and its authorized default workspace. User CLI identities include their verified email; CI
identities deliberately omit `email` and use the repository-bound machine label.

List responses use `{items, nextCursor}`. A non-null cursor is opaque and may only be passed back to
the same route and scope; clients must not parse or manufacture it.

Upload reservations accept optional `gitRepository`, `gitCommit`, and `gitBranch` provenance from
the native CLI. GitHub Actions provenance is always derived from the verified machine session, not
trusted from request fields. Inbox uploads cannot attach source-control provenance.

## Billing and account lifecycle routes

| Method | Route                             | Purpose                                                |
| ------ | --------------------------------- | ------------------------------------------------------ |
| POST   | `/v1/workspaces/rename`           | Rename an authorized workspace                         |
| GET    | `/v1/billing`                     | Read the current plan, storage, and usage projection   |
| POST   | `/v1/billing/checkout`            | Create a hosted paid-plan checkout session             |
| POST   | `/v1/billing/portal`              | Create a hosted billing management session             |
| POST   | `/v1/billing/storage/checkout`    | Create hosted checkout for managed storage             |
| POST   | `/v1/billing/storage/update`      | Update managed storage through hosted billing          |
| POST   | `/v1/billing/subscription/update` | Update the paid plan or Team seat count                |
| POST   | `/v1/account/exports`             | Queue a portable account data export                   |
| GET    | `/v1/account/exports`             | Read the current account export status                 |
| POST   | `/v1/account/exports/download`    | Issue a short-lived export download                    |
| POST   | `/v1/account/deletion/prepare`    | Return a short-lived confirmation capability once      |
| POST   | `/v1/account/deletion/complete`   | Consume that capability and queue asynchronous cleanup |
| GET    | `/v1/account/deletion`            | Read the current account deletion status               |
| POST   | `/v1/account/deletion/retry`      | Retry a failed account deletion job                    |

Deletion preparation does not suspend or delete the account. The confirmation expires after ten
minutes, is bound to the authenticated account, is stored only as a hash, and is replaced by a new
preparation. Completion consumes it once inside the same database transaction that starts deletion.
If deletion preconditions fail, the transaction rolls back so the still-valid confirmation can be
retried after the conflict is resolved.

Onboarding progress is derived browser UI state rather than a versioned resource. OpenAPI records
that classification explicitly and excludes it from SDK, CLI, and MCP generation.

## Web Yard routes

| Method | Route                        | Purpose                                                |
| ------ | ---------------------------- | ------------------------------------------------------ |
| POST   | `/v1/yards/deploys/start`    | Reserve a deployment and its immutable manifest        |
| POST   | `/v1/yards/deploys/finalise` | Verify uploaded files and make the deployment live     |
| POST   | `/v1/yards/deploys/fail`     | Record a bounded deployment failure                    |
| GET    | `/v1/yards`                  | List named Yards in the authorized project             |
| GET    | `/v1/yards/deploys`          | List immutable deployment history for one Yard         |
| POST   | `/v1/yards/rollback`         | Repoint the stable host to an earlier ready deployment |
| POST   | `/v1/yards/delete`           | Delete a Yard and schedule its retained bytes          |

`GET /v1/yards/resolve` is reserved for the Cloudflare edge and requires the server-only edge
credential. It is not a customer API. User HTML is returned only from isolated `blobyard.app` hosts,
never from the authenticated application origin.

## Errors

Stable codes include `INVALID_REQUEST`, `AUTH_REQUIRED`, `INVALID_TOKEN`, `TOKEN_EXPIRED`,
`FORBIDDEN`, `NOT_FOUND`, `CONFLICT`, `PLAN_LIMIT`, `UPLOAD_INCOMPLETE`, `CHECKSUM_MISMATCH`,
`RATE_LIMITED`, `PROVIDER_UNAVAILABLE`, and `INTERNAL_ERROR`.

Public capability failures intentionally make unknown, expired, and revoked resources difficult to
distinguish.
