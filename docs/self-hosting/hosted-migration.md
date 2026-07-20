# Migrate from Blob Yard Cloud

Blob Yard Core can import owned Cloud workspaces into a new standalone installation. Migration
requests a fresh account export, verifies every export artifact and immutable object, builds the
standalone metadata snapshot, uploads the verified bytes, and activates the destination only after
the complete import succeeds.

Migration does not delete or modify the Cloud account. It creates new standalone identities,
bootstrap authority, and share capabilities because Cloud credentials and capabilities are never
exported.

## Before you start

- Build or pull the exact Core version you intend to run.
- Create a new destination. The standalone data directory must not exist.
- For S3-compatible storage, use an empty bucket or empty prefix.
- Decide the public standalone origin before migration. Replacement share URLs use this origin.
- Keep the Cloud account active until the standalone installation is running and reconciles cleanly.

Create a short-lived Cloud API token with only the permissions migration needs:

```bash
blobyard tokens create "Core migration" \
  --expires-days 1 \
  --scope account:export \
  --scope object:read
```

The command prints the raw token once. Move it directly to a password manager. Do not put it in a
command argument, environment file, shell history, issue, log, or migration report.

## Migrate every owned workspace with Compose

Use one new Compose project name for the migration and the service that will run afterward. This
gives the migration a fresh named volume while preserving it for `compose up`:

```bash
export BLOBYARD_MIGRATION_PROJECT=blobyard-core-migration
umask 077
read -rs BLOBYARD_MIGRATION_TOKEN
printf '\n'

printf '%s' "$BLOBYARD_MIGRATION_TOKEN" | \
  docker compose \
    --project-name "$BLOBYARD_MIGRATION_PROJECT" \
    --file deploy/compose/filesystem.yaml \
    run --rm -T core \
      hosted-migrate \
      --token-stdin \
      --data-dir /var/lib/blobyard/data \
      --public-url http://localhost:8787 \
  >blobyard-migration-result.json

unset BLOBYARD_MIGRATION_TOKEN
```

The command reads the Cloud token only from standard input. It never accepts the token on the
command line. The result file is private operator material because it contains the new bootstrap
token and replacement share URLs. Store those values in a password manager before removing the local
result file.

To migrate only selected active workspaces, repeat `--workspace` with each exact slug:

```bash
      --workspace engineering \
      --workspace documentation
```

Omitting `--workspace` migrates every active workspace owned by the token principal.

## Migrate into MinIO or S3-compatible storage

For the bundled MinIO stack, set unique local credentials and use the same project name when you
start the service later:

```bash
export BLOBYARD_MIGRATION_PROJECT=blobyard-core-migration
export BLOBYARD_MINIO_ACCESS_KEY='replace-for-local-use'
export BLOBYARD_MINIO_SECRET_KEY='replace-with-a-long-random-secret'
umask 077
read -rs BLOBYARD_MIGRATION_TOKEN
printf '\n'

printf '%s' "$BLOBYARD_MIGRATION_TOKEN" | \
  docker compose \
    --project-name "$BLOBYARD_MIGRATION_PROJECT" \
    --file deploy/compose/minio.yaml \
    run --rm -T core \
      hosted-migrate \
      --token-stdin \
      --data-dir /var/lib/blobyard/data \
      --public-url http://localhost:8787 \
      --storage s3 \
      --s3-endpoint http://minio:9000 \
      --s3-bucket blobyard \
      --s3-force-path-style \
  >blobyard-migration-result.json

unset BLOBYARD_MIGRATION_TOKEN
```

For another S3-compatible provider, run `blobyard-server hosted-migrate` with that provider's
endpoint, region, bucket, optional prefix, and addressing mode. Supply credentials through
`BLOBYARD_S3_ACCESS_KEY_ID`, `BLOBYARD_S3_SECRET_ACCESS_KEY`, and, when required,
`BLOBYARD_S3_SESSION_TOKEN`.

## Verify and start the migrated installation

Inspect the result without copying its secret fields into logs. Confirm the workspace, project,
object-version, share-policy, and retention-policy counts match the intended source selection. Then
start the same Compose project:

```bash
docker compose \
  --project-name "$BLOBYARD_MIGRATION_PROJECT" \
  --file deploy/compose/filesystem.yaml \
  up --build --detach --wait

curl --fail http://127.0.0.1:8787/v1/health

docker compose \
  --project-name "$BLOBYARD_MIGRATION_PROJECT" \
  --file deploy/compose/filesystem.yaml \
  exec -T core \
    /usr/local/bin/blobyard-server reconcile \
    --data-dir /var/lib/blobyard/data
```

Use `deploy/compose/minio.yaml` and the matching S3 reconciliation flags for the MinIO stack. Do not
move traffic or distribute replacement share URLs until health is ready and reconciliation reports
`"clean":true` with zero findings.

Exchange the returned bootstrap token once through `POST /v1/bootstrap/exchange`, then revoke the
short-lived Cloud migration token after the standalone installation is proven. Cloud shares remain
unchanged. Any `shareUrls` in the migration result are new standalone capabilities and must be
distributed deliberately.

## Failure and retry rules

- A destination data directory that already exists is rejected before activation.
- A nonempty S3 bucket or prefix is rejected.
- Malformed exports, changed artifact metadata, unsafe download URLs, byte-size disagreement, and
  checksum disagreement fail closed.
- If object import fails, Blob Yard removes the objects written by that attempt. If provider cleanup
  also fails, do not reuse that bucket or prefix until its inventory is confirmed empty.
- A failed attempt does not produce a usable installation. Retry with a new absent data directory
  and, for S3-compatible storage, a confirmed empty bucket or prefix.
- Do not delete Cloud data until a backup of the standalone installation has been restored and
  reconciled successfully in isolation.
