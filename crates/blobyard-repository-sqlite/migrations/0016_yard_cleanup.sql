CREATE TABLE deletion_operations_v2 (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
  object_path TEXT NOT NULL,
  selected_version INTEGER CHECK(selected_version > 0),
  reason TEXT NOT NULL CHECK(reason IN ('object_delete', 'retention', 'yard_cleanup')),
  status TEXT NOT NULL CHECK(status IN ('pending', 'complete')),
  actor TEXT NOT NULL,
  request_id TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  completed_at_ms INTEGER,
  CHECK(
    (status = 'pending' AND completed_at_ms IS NULL)
    OR (status = 'complete' AND completed_at_ms IS NOT NULL)
  )
) STRICT;

CREATE TABLE deletion_items_v2 (
  operation_id TEXT NOT NULL REFERENCES deletion_operations_v2(id) ON DELETE CASCADE,
  version_id TEXT NOT NULL,
  storage_key TEXT NOT NULL,
  version INTEGER NOT NULL CHECK(version > 0),
  PRIMARY KEY(operation_id, version_id)
) STRICT;

INSERT INTO deletion_operations_v2
SELECT * FROM deletion_operations;

INSERT INTO deletion_items_v2
SELECT * FROM deletion_items;

DROP TABLE deletion_items;
DROP TABLE deletion_operations;

ALTER TABLE deletion_operations_v2 RENAME TO deletion_operations;
ALTER TABLE deletion_items_v2 RENAME TO deletion_items;

CREATE UNIQUE INDEX deletion_operations_logical_target
  ON deletion_operations(project_id, object_path)
  WHERE selected_version IS NULL AND reason = 'object_delete' AND status = 'pending';

CREATE UNIQUE INDEX deletion_operations_version_target
  ON deletion_operations(project_id, object_path, selected_version)
  WHERE selected_version IS NOT NULL AND reason = 'object_delete' AND status = 'pending';

CREATE UNIQUE INDEX deletion_operations_pending_retention
  ON deletion_operations(project_id)
  WHERE reason = 'retention' AND status = 'pending';

CREATE INDEX deletion_operations_pending_yard_cleanup
  ON deletion_operations(created_at_ms, id)
  WHERE reason = 'yard_cleanup' AND status = 'pending';
