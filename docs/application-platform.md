# Application platform contract

This document defines the application platform contract introduced by the vision change to complete
Yards: private, stateful small applications running on Blob Yard Core and Blob Yard Cloud. It is the
canonical written contract for entities that do not yet have OpenAPI operations. Operations, shared
component schemas, conformance fixtures, and handlers land together, stage by stage, exactly as Web
Yards did. The manifest schema in
[`openapi/blobyard-application-manifest.v1.schema.json`](../openapi/blobyard-application-manifest.v1.schema.json)
is normative today; `blobyard app validate` now provides the corresponding local validation tooling.

## The Yard model

A Yard is the deployable software object inside the existing workspace and project hierarchy. Static
Web Yards are the zero-capability configuration of the same model: they keep their identities,
stable URLs, and release history, and may grow stateful by adding an approved application manifest.
Yard kinds (static, app, automation) are labels over one domain model with declared capabilities,
never separate products.

| Entity              | Responsibility                                                                                    |
| ------------------- | ------------------------------------------------------------------------------------------------- |
| Yard                | Stable application identity, ownership, access policy, role mappings, release history, domains.   |
| Environment         | Isolated runtime target: active release, database, buckets, secrets, jobs, operational history.   |
| Release             | Immutable, content-addressed package: assets, functions, routes, migrations, manifest, checksums. |
| Deployment          | The validated, audited act of applying and activating one release in one environment.             |
| Resource capability | A declared and approved authority: database, bucket, secret, egress, email, or schedule access.   |

Every Yard has a production environment and may have staging and preview environments. Each
environment owns independent mutable state. Promotion moves an immutable release between
environments; it never copies mutable state. Deploying or rolling back code never silently alters
database or bucket contents, and rollback to a schema-incompatible release is blocked with an
actionable explanation.

## Capability approval

The manifest declares what an application requests. Yard policy records what the owner approved. A
release is never its own authorization, and deployment computes a capability diff against approved
policy:

| Change in a release                          | Handling                                         |
| -------------------------------------------- | ------------------------------------------------ |
| Code or asset changes within existing grants | Deploys under existing developer authority.      |
| New database or bucket capability            | Requires validation and owner approval.          |
| New secret name or new secret consumer       | Requires approval; values are never in releases. |
| New outbound network target or wider scope   | Requires explicit owner approval.                |
| New schedule, webhook, or background trigger | Requires approval: autonomous execution.         |
| Higher resource limits                       | Requires policy approval.                        |
| Public exposure or broader visibility        | Never a release change: access policy only.      |

## Access, identity, and permissions

Visibility modes: owner only, selected people and groups, workspace, authenticated link, any
authenticated user, public. Access changes are explicit policy operations with audit evidence, and
revocation is enforced promptly. Public Yards serve anonymously without consulting browser sessions.
Every other mode requires a live, host-bound Yard session and resolves the current user, policy,
grants, environment, and deployment on every request. `selected` and `authenticated-link` admit an
active same-workspace user through either a direct user grant or current membership in an actively
granted group; link redemption arrives later. `workspace` admits an active local user from the Yard
workspace, `any-authenticated` admits any active local user, and `owner` admits no browser-session
principal in Core. A navigation request without admission redirects to sign-in only for `GET` plus
`Accept: text/html`, including an unknown Yard-shaped host. Other requests remain concealed as not
found.

Two permission planes stay distinct everywhere. The management plane (owner, admin, developer,
auditor) controls who configures and operates the Yard. The application plane (roles the manifest
declares, granted only by the Yard owner) controls what an authenticated user may do inside the
application. Application code checks permissions; it can never grant them.

Core keeps control-plane authentication separate from Yard users. A non-machine operator credential
with `yard:manage` may bootstrap the first `owner` assignment only while a Yard has no owner.
Assignments target active local users in the same workspace, and changing or revoking the last owner
fails without mutation. The new management-role and application-policy operations reject CI
principals; existing deployment operations retain their existing CI authorization.

An approved application policy stores the canonical manifest role graph, its deterministic
transitive role and permission closure, the source-manifest digest, and a monotonically increasing
revision. New non-empty access-grant role arrays must name roles in that policy. Legacy unknown
roles remain visible in management reads but contribute no runtime authority. An empty role array
continues to grant admission without requiring an application policy.

Application code receives a sanitised identity, never platform account cookies or management
credentials:

```ts
type YardIdentity = {
  userId: string;
  workspaceId: string;
  projectId: string;
  yardId: string;
  environmentId: string;
  displayName: string | null;
  email: string | null;
  groups: string[];
  managementRole: "owner" | "admin" | "developer" | "auditor" | null;
  appRoles: string[];
  permissions: string[];
  sessionId: string;
};
```

Yard sessions are host- and environment-scoped, revocable, and stored only as hashes. A private Yard
redirects to the identity origin with a signed ten-minute continuation. Core verifies a local user's
`byuk_` sign-in key, evaluates the current policy, and issues a one-minute, host-bound, single-use
exchange code. The Yard origin consumes that code and sets a twelve-hour
`__Host-blobyard-yard-session` cookie with `Secure`, `HttpOnly`, `SameSite=Lax`, and `Path=/`. The
identity origin sets no cookie, so each Yard sign-in re-enters the key. Session claims are resolved
server-side on every delivery request; client-supplied tenant identifiers are never authorization
inputs, and revocation takes effect on the next request. Guest invitations, link redemption, and
OIDC identities arrive in later slices.

`GET /.blobyard/session/identity` returns the exact live `YardIdentity` on a private Yard origin
when the host-bound session remains admitted. It accepts only same-origin `GET`, emits
`Content-Type: application/json` and `Cache-Control: private, no-store`, and never exposes cookies
or other credential material. Public Yards, unknown hosts, invalid sessions, denied users, and
unsupported methods receive the same concealed not-found response, with no sign-in redirect and no
permissive CORS headers.

## Workspace groups

Workspace groups are human-managed collections of active local users. Group management requires a
human session with `users:manage`; machine identities cannot list or mutate groups. A group name is
stored in NFC after Unicode-whitespace trimming, contains no controls, and has 2-80 scalar values.
Core permits at most 500 active groups per workspace, 500 members per group, 100 memberships per
user, and 500 active Yard grants per group.

Group and member listings use opaque newest-first keyset cursors with pages of 50. Group listings
retain deactivated tombstones; member listings require an active group. Deactivating a group
atomically deletes all memberships, revokes every active grant for that group, and records one
`group.deactivated` audit event with `revokedGrantCount`. Deactivating a local user atomically
removes all of that user's memberships without revoking unrelated group grants or browser sessions.
Because admission is recalculated during issue, exchange, and every delivery, member removal, group
deactivation, or grant revocation denies the next request without a blanket session revoke.

Group IDs use `group_` followed by a lowercase UUID without separators. A group grant is accepted
only when the group is active and belongs to the Yard workspace. Legacy group-principal grants that
do not resolve to a current group remain stored but fail closed.

The generated `conformance/behavior/yard-sessions.json` and `conformance/authorization/vectors.json`
files carry the portable group/admission matrix for Core and Cloud. The Rust testkit asserts the
exact case inventory and execution owner. Core cases cover tenant isolation, lifecycle and
environment drift, deterministic pagination, cardinality limits, exact mutation audits, rollback,
and all seven machine-denied group routes. Better Auth workspace membership cases are marked
`conformanceOwner: cloud`; Core proves the corresponding local-user deactivation boundary without
claiming Cloud membership semantics.

## The application manifest

The authoring form is TOML. The canonical form is its direct JSON projection (same keys, same
snake_case names), validated by the versioned JSON Schema and used for signing and conformance.
Unknown fields fail closed by `schema_version`. Secret values, user data, and environment-specific
endpoints are never manifest content.

```toml
schema_version = 1

[application]
name = "engineering-risk-tracker"
runtime = "blobyard-js-1"

[frontend]
directory = "dist"
spa_fallback = true

[auth]
default_role = "viewer"

[auth.roles.viewer]
permissions = ["risks.read"]

[auth.roles.editor]
inherits = ["viewer"]
permissions = ["risks.write"]

[database]
migrations = "migrations"

[[buckets]]
name = "attachments"
max_object_size = "50MiB"

[[functions]]
name = "risk-create"
entry = "functions/risk-create.ts"
type = "rpc"
permissions = ["risks.write"]
database = "read-write"
buckets = ["attachments:read-write"]

[[functions]]
name = "weekly-summary"
entry = "functions/weekly-summary.ts"
type = "scheduled"
database = "read"
secrets = ["SUMMARY_EMAIL_FROM"]
network = ["api.example.com:443"]
email = true

[[jobs]]
name = "weekly-summary"
function = "weekly-summary"
schedule = "0 9 * * 1"
timezone = "Europe/London"

[health]
function = "health"
```

Rules the schema cannot express, enforced by manifest validation:

1. Every role referenced by `default_role`, `inherits`, or a function `permissions` entry is
   declared, and `inherits` chains are acyclic.
2. Every `function` referenced by a job, route, or health check is declared, and a job may only
   reference a `scheduled` function.
3. Every bucket grant references a declared bucket. `database` access requires a `[database]`
   section.
4. A function of type `event` declares `event`; a function of type `queue` declares `queue`; other
   types declare neither.
5. Route paths are unique per method, and `schedule` must parse as five-field cron with a valid IANA
   `timezone`.
6. Declared capabilities are the ceiling: a function receives only its own declared subset at
   runtime, as per-invocation, Yard-scoped capability handles.

## Runtime contract

Functions are deterministic ESM bundles running in an isolated runner boundary, never inside the API
server process, with no ambient filesystem, environment, process, or network access. Function types:
`rpc` (typed browser SDK calls), `http` (declared routes), `webhook` (externally addressable with
verification helpers and replay protection), `event`, `scheduled`, and `queue`. The runtime context
exposes only declared capabilities: `auth`, `db`, `files`, `secrets`, `fetch`, `jobs`, `events`,
`log`, and `email`.

The Yard Database is one isolated relational namespace per environment with SQLite-compatible
semantics, transactions, parameterized queries only, and forward-only, checksummed, release-owned
migrations. Buckets are named per-environment object stores over the existing storage foundation:
private by default, scoped direct transfers, signed downloads, and object lifecycle events. Durable
jobs are at-least-once with idempotency keys, bounded retries, dead letters, and manual replay.
Outbound networking is denied by default; declared targets pass through a controlled egress layer
that blocks private ranges, metadata endpoints, DNS rebinding, unsafe redirects, and oversized
responses.

## Deployment transaction

Deploying a release: authorize the actor, validate the package (schema, checksums, runtime version,
capability references), present the capability diff and collect required approvals, stage assets and
functions without changing live traffic, validate the migration plan, create the recovery
checkpoint, apply migrations under an environment deployment lock, run health checks in the staged
runtime, then activate atomically and start schedules. Any pre-activation failure leaves the
previous release live and the failed deployment inspectable. Every step records audit evidence.

## Portability

A complete Yard exports and imports between Cloud and Core: metadata, manifest, role mappings,
releases, database export with migration history, bucket manifests with verified byte transfer,
schedules, domains, non-sensitive configuration, and secret names without values. Import fails
closed on identity ambiguity, checksum mismatch, unsupported runtime version, incompatible schema,
or incomplete transfer. Conformance extends beyond management endpoints to runtime behavior:
authorization vectors, error codes, database semantics, session flow, job retries, capability
approval, and deployment state transitions.
