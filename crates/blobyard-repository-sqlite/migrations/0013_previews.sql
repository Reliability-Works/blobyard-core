CREATE TABLE previews (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
  capability_hash TEXT NOT NULL UNIQUE,
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms >= 0),
  status TEXT NOT NULL CHECK(status IN ('active', 'revoked')),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  revoked_at_ms INTEGER,
  CHECK(expires_at_ms > created_at_ms),
  CHECK(
    (status = 'revoked' AND revoked_at_ms IS NOT NULL AND revoked_at_ms >= created_at_ms)
    OR (status = 'active' AND revoked_at_ms IS NULL)
  )
) STRICT;

CREATE INDEX previews_project_created
  ON previews(project_id, created_at_ms DESC, id DESC);

CREATE INDEX previews_capability
  ON previews(capability_hash, expires_at_ms, status);

CREATE TABLE preview_files (
  preview_id TEXT NOT NULL REFERENCES previews(id) ON DELETE CASCADE,
  normalized_path TEXT NOT NULL,
  version_id TEXT REFERENCES object_versions(id) ON DELETE SET NULL,
  PRIMARY KEY(preview_id, normalized_path)
) STRICT;
