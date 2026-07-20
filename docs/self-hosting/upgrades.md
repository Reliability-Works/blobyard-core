# Self-hosted upgrades and rollback

Blob Yard Core uses forward-only numbered `SQLite` migrations. An upgrade may move metadata to a
newer schema. Code rollback is safe only when the older binary supports the installation's exact
current schema. Blob Yard never migrates production metadata backward.

## Upgrade procedure

1. Run the candidate binary's read-only preflight against the current installation:

   ```bash
   /opt/blobyard-candidate/bin/blobyard-server upgrade-preflight \
     --data-dir /srv/blobyard
   ```

2. Confirm the JSON report identifies the installation as compatible. The report always records
   `"backupRequired":true`.
3. Create and retain a verified backup with the currently running binary:

   ```bash
   /opt/blobyard-current/bin/blobyard-server backup \
     --data-dir /srv/blobyard \
     --output /srv/backups/pre-upgrade-2026-07-19
   ```

4. Stop traffic and the current process cleanly. Do not run two server versions against one data
   directory.
5. Install the candidate binary, start it against the existing data directory, and wait for the
   health endpoint to report ready.
6. Run read-only reconciliation with the same storage settings used by the service:

   ```bash
   blobyard-server reconcile --data-dir /srv/blobyard
   ```

7. Resume traffic only when health is ready and reconciliation reports `"clean":true`.

If preflight fails, do not start the candidate. If backup fails, do not upgrade. If health or
reconciliation fails after startup, stop traffic and use the recovery decision below.

## Decide whether code rollback is safe

Run the previous binary's rollback preflight against the current installation:

```bash
/opt/blobyard-previous/bin/blobyard-server rollback-preflight \
  --data-dir /srv/blobyard
```

This succeeds only when the installation's schema exactly matches the previous binary's current
schema. A successful result allows a code-only rollback: stop the candidate, start the previous
binary, verify health, and run reconciliation.

If rollback preflight reports an older or newer schema, do not start the previous binary against
that data directory.

## Recover when the schema advanced

Schema rollback means restoring the pre-upgrade backup, not migrating the live database backward:

1. Keep the upgraded installation and storage namespace intact for incident inspection.
2. Provision a separate absent data directory and, for S3-compatible storage, a separate empty
   bucket or prefix.
3. Use the previous binary to restore the pre-upgrade backup into that empty installation.
4. Run the previous binary's rollback preflight against the restored data directory.
5. Start the restored installation in isolation, verify health, and require clean reconciliation.
6. Move traffic only after the restored installation is proven healthy.

Never overwrite the upgraded data directory, reuse a nonempty object prefix, edit `user_version`, or
run reverse SQL migrations. Keeping the failed installation separate preserves evidence and keeps
recovery reversible until traffic moves.

## Operational constraints

- Keep the previous binary and its checksums until the new version has passed the full observation
  window.
- Keep the pre-upgrade backup until a newer recovery point has been restored successfully in an
  isolated environment.
- Use the same object-storage endpoint, region, bucket, prefix, and addressing mode for serve,
  backup, reconciliation, and retention commands.
- Record the binary version, backup path, schema version, health result, reconciliation result, and
  traffic-move time in the upgrade log.
- Never treat a code deployment as a data rollback. They are separate decisions with separate
  preconditions.
