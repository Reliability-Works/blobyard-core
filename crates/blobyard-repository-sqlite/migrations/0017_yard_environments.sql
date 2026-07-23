CREATE TABLE yard_environments (
  id TEXT PRIMARY KEY NOT NULL,
  yard_id TEXT NOT NULL REFERENCES web_yards(id) ON DELETE RESTRICT,
  name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('production', 'staging', 'preview')),
  status TEXT NOT NULL CHECK(status IN ('active', 'deleted')),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
  deleted_at_ms INTEGER,
  CHECK(
    (status = 'deleted' AND deleted_at_ms IS NOT NULL AND deleted_at_ms >= created_at_ms)
    OR (status != 'deleted' AND deleted_at_ms IS NULL)
  )
) STRICT;

CREATE UNIQUE INDEX yard_environments_active_yard_name
  ON yard_environments(yard_id, name)
  WHERE status != 'deleted';

INSERT INTO yard_environments (id, yard_id, name, kind, status, created_at_ms, updated_at_ms, deleted_at_ms)
SELECT 'yardenv_' || id, id, 'production', 'production', 'active', created_at_ms, created_at_ms, NULL
FROM web_yards
WHERE status != 'deleted';
