ALTER TABLE upload_reservations
  ADD COLUMN strategy TEXT NOT NULL DEFAULT 'single'
  CHECK(strategy IN ('single', 'multipart'));

ALTER TABLE upload_reservations
  ADD COLUMN part_size INTEGER CHECK(part_size IS NULL OR part_size > 0);

ALTER TABLE upload_reservations
  ADD COLUMN part_count INTEGER CHECK(part_count IS NULL OR (part_count > 0 AND part_count <= 10000));

ALTER TABLE upload_reservations
  ADD COLUMN provider_upload_id TEXT;

CREATE TABLE upload_parts (
  upload_id TEXT NOT NULL REFERENCES upload_reservations(id) ON DELETE CASCADE,
  part_number INTEGER NOT NULL CHECK(part_number > 0 AND part_number <= 10000),
  expected_size INTEGER NOT NULL CHECK(expected_size > 0),
  capability_hash TEXT NOT NULL UNIQUE,
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms >= 0),
  state TEXT NOT NULL CHECK(state IN ('pending', 'uploaded')),
  received_size INTEGER,
  received_checksum TEXT,
  PRIMARY KEY(upload_id, part_number),
  CHECK(
    (state = 'pending' AND received_size IS NULL AND received_checksum IS NULL)
    OR (state = 'uploaded' AND received_size IS NOT NULL AND received_checksum IS NOT NULL)
  )
) STRICT;

CREATE INDEX upload_parts_capability
  ON upload_parts(capability_hash, expires_at_ms, state);
