# Self-hosted quickstart

Blob Yard Core runs as one server with SQLite metadata and either filesystem or S3-compatible object
storage. These Compose examples bind only to `127.0.0.1:8787`, persist data in named volumes, drop
Linux capabilities, use a read-only container filesystem, and run the same binary-owned readiness
check used by operator acceptance.

## Requirements

- Docker with Compose v2
- `curl`
- Ports `8787` on the local machine, or set `BLOBYARD_CORE_PORT` to another unused port

Run every command from the Core repository root.

## Start with filesystem storage

Generate the first-start bootstrap authority before starting the service:

```bash
docker compose --file deploy/compose/filesystem.yaml run --rm core \
  bootstrap-token --generate --data-dir /var/lib/blobyard/data
```

The command prints one `byb_` bootstrap token exactly once. Copy it directly into your password
manager. Do not paste it into chat, issue trackers, shell history, screenshots, or shared logs. The
server stores only the token hash, so the raw value cannot be recovered later.

Start the service and wait for readiness:

```bash
docker compose --file deploy/compose/filesystem.yaml up --build --detach --wait
curl --fail http://127.0.0.1:8787/v1/health
```

If you changed the host port, use the same value in the request:

```bash
export BLOBYARD_CORE_PORT=8877
docker compose --file deploy/compose/filesystem.yaml up --build --detach --wait
curl --fail "http://127.0.0.1:${BLOBYARD_CORE_PORT}/v1/health"
```

Exchange the bootstrap token once through `POST /v1/bootstrap/exchange`, then use the returned CLI
session credential for normal administration. Consuming or revoking that authority does not print
the original bootstrap token again.

## Start with MinIO

The MinIO example creates one private `blobyard` bucket and keeps SQLite metadata in a separate Core
volume. Its default credentials are only for local evaluation. Set unique values before using a
shared machine:

```bash
export BLOBYARD_MINIO_ACCESS_KEY='replace-for-local-use'
export BLOBYARD_MINIO_SECRET_KEY='replace-with-a-long-random-secret'

docker compose --file deploy/compose/minio.yaml run --rm core \
  bootstrap-token --generate --data-dir /var/lib/blobyard/data
docker compose --file deploy/compose/minio.yaml up --build --detach --wait
curl --fail http://127.0.0.1:8787/v1/health
```

The server receives S3 credentials through environment variables. They are not written to SQLite,
the runtime secret, reconciliation reports, or backup manifests.

## Verify the installation

Run the binary-owned readiness check inside the service:

```bash
docker compose --file deploy/compose/filesystem.yaml exec -T core \
  /usr/local/bin/blobyard-server healthcheck \
  --url http://127.0.0.1:8787/v1/health
```

Run read-only reconciliation and require both `"clean":true` and zero findings:

```bash
docker compose --file deploy/compose/filesystem.yaml exec -T core \
  /usr/local/bin/blobyard-server reconcile --data-dir /var/lib/blobyard/data
```

For MinIO, include the exact storage configuration used by the server:

```bash
docker compose --file deploy/compose/minio.yaml exec -T core \
  /usr/local/bin/blobyard-server reconcile \
  --data-dir /var/lib/blobyard/data \
  --storage s3 \
  --s3-endpoint http://minio:9000 \
  --s3-bucket blobyard \
  --s3-force-path-style
```

## Web Yards on localhost

The Compose service uses `http://localhost:8787` as its Web Yard origin. A deployed Yard receives a
first-level host such as `documentation-123456789-main.localhost:8787`. Modern browsers resolve
`*.localhost` to the local machine, so public Yard content remains isolated from the API host while
requiring no local DNS changes.

Production installations must point `--web-yard-origin` at a dedicated origin whose first-level
subdomains resolve to the server. Do not serve user HTML from the authenticated application origin.

For a private Yard, create a local user, retain the returned `byuk_` sign-in key, grant that user
access, and change the Yard visibility:

```bash
blobyard --profile local users create "Yard Reader" --workspace default
blobyard --profile local access grant documentation \
  --principal-kind user --principal-id user_123
blobyard --profile local access set-visibility documentation selected
```

Open the stable Yard URL in a browser and enter the sign-in key. Core returns through the exact Yard
host and stores a twelve-hour, host-only HttpOnly session cookie. Access changes and user
deactivation take effect on the next request. Inspect or revoke retained sessions without exposing
cookie material:

```bash
blobyard --profile local yard-sessions list documentation
blobyard --profile local yard-sessions revoke documentation yardsession_123
```

## Optional generic OIDC sign-in

Core can add a generic OpenID Connect provider to the existing private-Yard sign-in page. Register
this exact redirect URI with the provider, using the same root origin supplied to `--public-url`:

```text
https://core.example.com/account/yard-oidc/callback
```

Supply the non-secret issuer and client identifier on the `serve` command, and supply the client
secret only through the process environment:

```bash
export BLOBYARD_OIDC_CLIENT_SECRET='replace-with-the-provider-client-secret'

blobyard-server serve \
  --listen 127.0.0.1:8787 \
  --data-dir /srv/blobyard \
  --public-url https://core.example.com \
  --web-yard-origin https://yards.example.com \
  --oidc-issuer https://identity.example.com/realms/blobyard \
  --oidc-client-id blobyard-core
```

The issuer, client identifier, and `BLOBYARD_OIDC_CLIENT_SECRET` are all-or-nothing. Core completes
provider discovery and validates the authorization, token, key, and optional UserInfo endpoint URLs
before opening its listener. A discovery or configuration failure therefore stops startup rather
than silently removing sign-in. The callback origin derived from `--public-url` must use HTTPS,
except that loopback HTTP origins are accepted for local development and tests.

OIDC does not provision users or grant access. The provider must return a verified email that
matches exactly one pre-existing active local user in the Yard workspace or one accepted guest whose
grant remains valid. Core then binds the exact issuer and provider subject to that existing
identity. A missing verified email or later email drift denies sign-in and revokes that identity's
active Yard sessions. Keep local-user and guest lifecycle administration in the existing CLI and API
paths.

Local-user emails are stored trimmed and lowercased so they compare exactly against the normalized
provider email. Upgrading to this release normalizes stored local-user emails. The upgrade fails
closed and leaves the database untouched when two active users in one workspace normalize to the
same address; resolve the duplicate by deactivating one of the users, then start the server again.

The browser flow requests only `openid`, `email`, and `profile`, uses authorization code flow with
PKCE and a nonce, and stores only hashed single-use state. Provider tokens, the client secret, raw
state, nonce, and PKCE verifier are not written to SQLite. These account routes are browser-only
identity surfaces and are intentionally absent from OpenAPI, the SDK, CLI operations, and MCP.

## Restart and inspect

Restart Core without removing its durable volume:

```bash
docker compose --file deploy/compose/filesystem.yaml restart core
docker compose --file deploy/compose/filesystem.yaml up --detach --wait
```

For MinIO restart both services, then wait for the bucket initializer and Core readiness:

```bash
docker compose --file deploy/compose/minio.yaml restart core minio
docker compose --file deploy/compose/minio.yaml up --detach --wait
```

After every restart, check health and run reconciliation. Startup also resumes any durable object
deletion plan that was interrupted after metadata planning but before byte or metadata completion.

## Backup, restore, and upgrade

Before upgrading or moving storage, follow [backup and restore](./backup-restore.md),
[hosted migration](./hosted-migration.md), and [upgrades and rollback](./upgrades.md). A successful
health response is not a substitute for a verified backup and clean reconciliation report.

## Stop or remove

Stop containers while retaining data:

```bash
docker compose --file deploy/compose/filesystem.yaml down
```

Removing named volumes permanently deletes the local installation. Back up and verify recovery
before running this command:

```bash
docker compose --file deploy/compose/filesystem.yaml down --volumes
```

Use `deploy/compose/minio.yaml` in the same commands for the MinIO stack. Its volume removal deletes
both Core metadata and MinIO object bytes.

## Repeatable operator acceptance

The repository acceptance script builds both stacks in isolated Compose projects, checks health,
requires clean reconciliation, restarts the dependencies, checks them again, and removes only its
own disposable volumes:

```bash
scripts/open-core/test-compose.sh
```

The script intentionally does not print bootstrap logs or token values.
