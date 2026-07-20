CREATE TABLE upload_reservations (
  id TEXT PRIMARY KEY NOT NULL,
  version_id TEXT NOT NULL UNIQUE REFERENCES object_versions(id) ON DELETE RESTRICT,
  filename TEXT NOT NULL,
  content_type TEXT NOT NULL,
  expected_size INTEGER NOT NULL CHECK(expected_size >= 0),
  expected_checksum TEXT NOT NULL,
  capability_hash TEXT NOT NULL UNIQUE,
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms >= 0),
  state TEXT NOT NULL CHECK(state IN ('requested', 'uploaded', 'complete', 'aborted')),
  received_size INTEGER,
  received_checksum TEXT,
  CHECK(
    (state IN ('requested', 'aborted') AND received_size IS NULL AND received_checksum IS NULL)
    OR (state IN ('uploaded', 'complete') AND received_size IS NOT NULL AND received_checksum IS NOT NULL)
  )
) STRICT;

CREATE INDEX upload_reservations_capability
  ON upload_reservations(capability_hash, expires_at_ms, state);
