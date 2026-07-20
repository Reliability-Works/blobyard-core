CREATE TABLE shares (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  version_id TEXT REFERENCES object_versions(id) ON DELETE SET NULL,
  capability_hash TEXT NOT NULL UNIQUE,
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms >= 0),
  status TEXT NOT NULL CHECK(status IN ('active', 'exhausted', 'revoked')),
  consumed_count INTEGER NOT NULL DEFAULT 0 CHECK(consumed_count >= 0),
  maximum_downloads INTEGER CHECK(maximum_downloads > 0),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  revoked_at_ms INTEGER,
  CHECK(expires_at_ms > created_at_ms),
  CHECK(
    (status = 'revoked' AND revoked_at_ms IS NOT NULL AND revoked_at_ms >= created_at_ms)
    OR (status != 'revoked' AND revoked_at_ms IS NULL)
  ),
  CHECK(maximum_downloads IS NULL OR consumed_count <= maximum_downloads),
  CHECK(status != 'exhausted' OR consumed_count = maximum_downloads)
) STRICT;

CREATE INDEX shares_workspace_created
  ON shares(workspace_id, created_at_ms DESC, id DESC);

CREATE INDEX shares_capability
  ON shares(capability_hash, expires_at_ms, status);
