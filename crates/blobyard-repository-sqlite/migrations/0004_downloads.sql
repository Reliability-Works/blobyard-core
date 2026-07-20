ALTER TABLE object_versions
  ADD COLUMN created_at_ms INTEGER NOT NULL DEFAULT 0 CHECK(created_at_ms >= 0);

CREATE TABLE download_grants (
  capability_hash TEXT PRIMARY KEY NOT NULL,
  version_id TEXT NOT NULL REFERENCES object_versions(id) ON DELETE CASCADE,
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms >= 0)
) STRICT;

CREATE INDEX download_grants_expiry
  ON download_grants(expires_at_ms);
