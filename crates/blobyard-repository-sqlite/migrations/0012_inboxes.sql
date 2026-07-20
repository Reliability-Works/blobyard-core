CREATE TABLE inboxes (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
  name TEXT NOT NULL,
  capability_hash TEXT NOT NULL UNIQUE,
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms >= 0),
  status TEXT NOT NULL CHECK(status IN ('active', 'revoked')),
  current_files INTEGER NOT NULL DEFAULT 0 CHECK(current_files >= 0),
  current_bytes INTEGER NOT NULL DEFAULT 0 CHECK(current_bytes >= 0),
  reserved_files INTEGER NOT NULL DEFAULT 0 CHECK(reserved_files >= 0),
  reserved_bytes INTEGER NOT NULL DEFAULT 0 CHECK(reserved_bytes >= 0),
  maximum_files INTEGER NOT NULL CHECK(maximum_files > 0),
  maximum_bytes INTEGER NOT NULL CHECK(maximum_bytes > 0),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  revoked_at_ms INTEGER,
  CHECK(expires_at_ms > created_at_ms),
  CHECK(current_files + reserved_files <= maximum_files),
  CHECK(current_bytes + reserved_bytes <= maximum_bytes),
  CHECK(
    (status = 'revoked' AND revoked_at_ms IS NOT NULL AND revoked_at_ms >= created_at_ms)
    OR (status = 'active' AND revoked_at_ms IS NULL)
  )
) STRICT;

CREATE INDEX inboxes_project_created
  ON inboxes(project_id, created_at_ms DESC, id DESC);

CREATE INDEX inboxes_capability
  ON inboxes(capability_hash, expires_at_ms, status);

CREATE TABLE inbox_uploads (
  upload_id TEXT PRIMARY KEY NOT NULL REFERENCES upload_reservations(id) ON DELETE CASCADE,
  inbox_id TEXT NOT NULL REFERENCES inboxes(id) ON DELETE RESTRICT,
  fingerprint_hash TEXT NOT NULL,
  reserved_size INTEGER NOT NULL CHECK(reserved_size >= 0),
  status TEXT NOT NULL CHECK(status IN ('reserved', 'complete', 'aborted')),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0)
) STRICT;

CREATE INDEX inbox_uploads_inbox_created
  ON inbox_uploads(inbox_id, created_at_ms DESC, upload_id DESC);

CREATE TABLE inbox_rate_limits (
  rate_key TEXT PRIMARY KEY NOT NULL,
  window_started_at_ms INTEGER NOT NULL CHECK(window_started_at_ms >= 0),
  request_count INTEGER NOT NULL CHECK(request_count > 0)
) STRICT;
