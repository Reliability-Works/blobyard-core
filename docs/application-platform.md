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
revocation is enforced promptly.

Two permission planes stay distinct everywhere. The management plane (owner, admin, developer,
auditor) controls who configures and operates the Yard. The application plane (roles the manifest
declares, granted only by the Yard owner) controls what an authenticated user may do inside the
application. Application code checks permissions; it can never grant them.

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

Yard sessions are host- and environment-scoped, short-lived, revocable, and HttpOnly. A private Yard
redirects through the configured identity provider with a signed, single-use continuation, and a
short-lived single-use code is exchanged for the session on the Yard origin. Session claims are
resolved server-side; client-supplied tenant identifiers are never authorization inputs. Core
provides local users, groups, guest invitations, and generic OIDC.

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
