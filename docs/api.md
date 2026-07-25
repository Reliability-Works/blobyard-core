# Blobyard HTTP API

The production API is `https://api.blobyard.com/v1`. Development exposes the same versioned contract
through the edge Worker.

The currently deployed machine-readable route and authentication inventory is available at
[`https://blobyard.com/openapi.json`](https://blobyard.com/openapi.json). The checked-in release
candidate contract contains 92 public operations. It remains a candidate until deployment and hosted
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
| Administration      | audit, members, invites, API tokens, local users, groups, CI trusts, and CLI sessions                           |
| Billing             | hosted paid-plan checkout and billing portal sessions                                                           |
| Account lifecycle   | portable export plus two-phase account deletion                                                                 |
| Public utility      | client-encrypted one-time secret create and redeem                                                              |

For production clients, the hosted OpenAPI document at `https://blobyard.com/openapi.json` is the
authoritative inventory of deployed public methods and paths. The checked-in 92-operation release
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
plus access to the named workspace. Local-user and group management require a human principal with
`users:manage`; machine identities are rejected.

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

| Method | Route                         | Purpose                                                 |
| ------ | ----------------------------- | ------------------------------------------------------- |
| POST   | `/v1/yards/deploys/start`     | Reserve a deployment and its immutable manifest         |
| POST   | `/v1/yards/deploys/finalise`  | Verify uploaded files and make the deployment live      |
| POST   | `/v1/yards/deploys/fail`      | Record a bounded deployment failure                     |
| GET    | `/v1/yards`                   | List named Yards in the authorized project              |
| GET    | `/v1/yards/deploys`           | List immutable deployment history for one Yard          |
| GET    | `/v1/yards/environments`      | List active environments for one Yard                   |
| GET    | `/v1/yards/access`            | Read one Yard's effective visibility and active grants  |
| POST   | `/v1/yards/access/visibility` | Set one Yard's visibility policy (human sessions only)  |
| POST   | `/v1/yards/access/grant`      | Grant one principal scoped access (human sessions only) |
| POST   | `/v1/yards/access/revoke`     | Revoke one access grant (human sessions only)           |
| GET    | `/v1/yards/sessions`          | List retained browser sessions for one Yard             |
| POST   | `/v1/yards/sessions/revoke`   | Revoke one browser session (human sessions only)        |
| POST   | `/v1/yards/rollback`          | Repoint the stable host to an earlier ready deployment  |
| POST   | `/v1/yards/delete`            | Delete a Yard and schedule its retained bytes           |

## Yard sessions and the browser flow

Public Yards serve without consulting a session. A non-public navigation request, defined as `GET`
with an `Accept` value containing `text/html`, redirects from the exact Yard host to
`GET /account/yard-login` on the configured identity origin. Other methods, `HEAD`, and non-HTML
requests remain concealed as not found. Unknown Yard-shaped hosts use the same redirect shape, so
the response does not disclose whether a Yard exists.

The identity route verifies a signed ten-minute continuation and accepts a local user's raw `byuk_`
sign-in key through an `application/x-www-form-urlencoded` POST. It evaluates the current Yard
policy and returns a one-minute, single-use `byx_` exchange code to
`GET /.blobyard/session/exchange` on the exact Yard origin. The Yard origin consumes the code
atomically and sets `__Host-blobyard-yard-session` with `Secure`, `HttpOnly`, `SameSite=Lax`, and
`Path=/`. The absolute session lifetime is twelve hours. The identity origin sets no cookie.

Every private delivery request resolves the hashed session token, active local user, exact host,
environment, current deployment, policy, and grants in one live repository path. Revoking a session
or grant, tightening visibility, deactivating the user, deleting the Yard, or expiring a grant
therefore denies the next request. `owner` admits no browser-session principal in Core. `selected`
and `authenticated-link` accept either a direct user grant or an active grant held by one of the
user's current groups; link redemption arrives later. `POST /.blobyard/session/logout` requires a
matching Origin when supplied, revokes the current session, and clears the cookie idempotently.

`GET /v1/yards/sessions` requires Yard read authority and returns retained metadata without raw
tokens. `POST /v1/yards/sessions/revoke` requires a human Yard manager, is idempotent for an already
revoked session, and records `yard.session_revoked`. Both operations conceal cross-workspace Yard
and session identifiers.

## Local user routes

| Method | Route                  | Purpose                                                           |
| ------ | ---------------------- | ----------------------------------------------------------------- |
| GET    | `/v1/users`            | List local users with sign-in key prefixes (human sessions only)  |
| POST   | `/v1/users`            | Create a local user, returning the key once (human sessions only) |
| POST   | `/v1/users/reset-key`  | Replace every active sign-in key at once (human sessions only)    |
| POST   | `/v1/users/deactivate` | Deactivate a user and revoke its keys (human sessions only)       |

Local users are the self-hosted identities behind non-public Yard access. These four operations are
available only in self-hosted Core and are omitted from the Blob Yard Cloud contract, where Better
Auth owns identity. They require the operator scope `users:manage` and reject machine principals.
Raw `byuk_` sign-in keys are returned exactly once from create and reset-key, stored only as SHA-256
digests, and never appear in listings, audit events, or logs; listings expose only the non-secret
key prefix. Deactivation is a tombstone: it revokes every active sign-in key and Yard browser
session in the same transaction, removes every group membership, and answers `CONFLICT` when
repeated.

## Workspace group routes

| Method | Route                       | Purpose                                                         |
| ------ | --------------------------- | --------------------------------------------------------------- |
| GET    | `/v1/groups`                | List active and deactivated groups with an opaque cursor        |
| POST   | `/v1/groups`                | Create an empty active group                                    |
| POST   | `/v1/groups/rename`         | Rename an active group                                          |
| GET    | `/v1/groups/members`        | List the current members of an active group                     |
| POST   | `/v1/groups/members`        | Add an active same-workspace local user                         |
| POST   | `/v1/groups/members/remove` | Remove a current member                                         |
| POST   | `/v1/groups/deactivate`     | Tombstone a group, remove members, and revoke its active grants |

All group routes require a human `users:manage` principal. Names are NFC-normalized,
Unicode-whitespace-trimmed, control-free, and 2-80 scalar values. Listings return 50 newest-first
records per page; cursors are bound to the exact workspace or group. Deactivation is one atomic
transaction and records a single audit event including the number of grants revoked.

Group Yard grants resolve only for active groups in the same workspace. The browser admission path
checks direct grants and current group membership during continuation issue, exchange, and every
delivery request. Removing a membership, deactivating a group, or revoking its grant therefore
denies the next request without revoking unrelated sessions. Unresolved legacy group grants are
preserved during migration and fail closed.

`GET /v1/yards/resolve` is reserved for the Cloudflare edge and requires the server-only edge
credential. It is not a customer API. User HTML is returned only from isolated `blobyard.app` hosts,
never from the authenticated application origin.

## Errors

Stable codes include `INVALID_REQUEST`, `AUTH_REQUIRED`, `INVALID_TOKEN`, `TOKEN_EXPIRED`,
`FORBIDDEN`, `NOT_FOUND`, `CONFLICT`, `PLAN_LIMIT`, `UPLOAD_INCOMPLETE`, `CHECKSUM_MISMATCH`,
`RATE_LIMITED`, `PROVIDER_UNAVAILABLE`, and `INTERNAL_ERROR`.

Public capability failures intentionally make unknown, expired, and revoked resources difficult to
distinguish.
