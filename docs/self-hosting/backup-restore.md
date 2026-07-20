# Self-hosted backup and restore

Blob Yard Core backups are portable directories containing one consistent metadata snapshot, the
runtime secret, every referenced complete object, and a strict checksummed manifest. Treat the
backup directory as sensitive: it contains private object bytes and the authority needed by the
restored installation.

## Create a filesystem backup

Choose a new destination path. Blob Yard never replaces an existing path.

```bash
blobyard-server backup \
  --data-dir /srv/blobyard \
  --output /srv/backups/blobyard-2026-07-19
```

The command performs an online `SQLite` snapshot, rejects unsupported schemas and pending uploads,
copies each complete object, verifies its size and SHA-256 checksum, writes `manifest.json`, then
atomically publishes the completed backup directory. Any database, storage, integrity, or
persistence failure leaves no successful backup at the requested destination.

A successful JSON report has `"operation":"backup"` and `"destinationReady":true`. Preserve the
whole directory together:

```text
manifest.json
metadata.sqlite3
runtime.secret
objects/<storage-key>
```

## Create an S3-compatible backup

The backup format is identical for filesystem and S3-compatible installations. Credentials are read
from the environment and never written to the backup manifest or command output.

```bash
export BLOBYARD_S3_ACCESS_KEY_ID='...'
export BLOBYARD_S3_SECRET_ACCESS_KEY='...'

blobyard-server backup \
  --data-dir /srv/blobyard \
  --output /srv/backups/blobyard-2026-07-19 \
  --storage s3 \
  --s3-endpoint https://s3.example.com \
  --s3-region eu-west-1 \
  --s3-bucket blobyard-core \
  --s3-prefix production
```

Use `BLOBYARD_S3_SESSION_TOKEN` when the provider issues temporary credentials. Add
`--s3-force-path-style` only when the provider requires path-style addressing, such as a typical
local MinIO setup.

## Restore into a new filesystem installation

Restore never overlays an installation. The target data directory must not exist.

```bash
blobyard-server restore \
  --input /srv/backups/blobyard-2026-07-19 \
  --data-dir /srv/blobyard-restored
```

Before activation, Blob Yard validates the manifest shape, supported schema, control-file hashes,
database integrity, runtime secret, every object path, every object checksum, and the total byte
count. It stages control files privately and publishes the new installation only after all objects
have been restored and the physical inventory exactly matches the manifest.

A successful JSON report has `"operation":"restore"` and `"installationReady":true`. Start the
restored service only after the command succeeds, then run reconciliation:

```bash
blobyard-server reconcile --data-dir /srv/blobyard-restored
```

The reconciliation report must contain `"clean":true` before the installation receives traffic.

## Restore into S3-compatible storage

Use a dedicated empty bucket or an empty prefix. The local data directory must also be absent.

```bash
blobyard-server restore \
  --input /srv/backups/blobyard-2026-07-19 \
  --data-dir /srv/blobyard-restored \
  --storage s3 \
  --s3-endpoint https://s3.example.com \
  --s3-region eu-west-1 \
  --s3-bucket blobyard-core-restored \
  --s3-prefix recovery-2026-07-19
```

Blob Yard refuses a nonempty storage namespace. If object import or local activation fails, it
deletes objects imported by that restore attempt. If provider cleanup itself fails, the command
returns a storage failure. Do not reuse that prefix until its inventory is empty and the incident
has been inspected.

## Failure and retry rules

- Never edit `manifest.json`, the metadata snapshot, the runtime secret, or object paths.
- Never copy only part of a backup directory.
- A failed backup is not a recovery point. Resolve the reported cause and create a new backup at a
  new destination.
- A failed filesystem restore can be retried after confirming the target data directory is absent.
- A failed S3 restore can be retried only with a confirmed empty bucket or prefix.
- Keep at least one verified backup outside the installation's host and storage account failure
  boundary.
- Test restoration regularly into an isolated installation. A backup is useful only when its full
  restore and reconciliation journey succeeds.
