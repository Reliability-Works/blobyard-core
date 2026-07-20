ALTER TABLE object_versions
  ADD COLUMN git_branch TEXT;

CREATE TABLE retention_policies (
  project_id TEXT PRIMARY KEY NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
  keep_latest INTEGER NOT NULL CHECK(keep_latest > 0),
  path_glob TEXT,
  branch_glob TEXT,
  enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms)
) STRICT;

CREATE TABLE retention_runs (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
  candidate_count INTEGER NOT NULL CHECK(candidate_count >= 0),
  deleted_count INTEGER NOT NULL CHECK(deleted_count >= 0 AND deleted_count <= candidate_count),
  status TEXT NOT NULL CHECK(status IN ('running', 'complete', 'failed')),
  started_at_ms INTEGER NOT NULL CHECK(started_at_ms >= 0),
  completed_at_ms INTEGER,
  error_summary TEXT,
  CHECK(
    (status = 'running' AND completed_at_ms IS NULL AND error_summary IS NULL)
    OR (status = 'complete' AND completed_at_ms IS NOT NULL AND error_summary IS NULL)
    OR (status = 'failed' AND completed_at_ms IS NOT NULL AND error_summary IS NOT NULL)
  )
) STRICT;

CREATE INDEX retention_runs_project_started
  ON retention_runs(project_id, started_at_ms DESC, id DESC);

CREATE TABLE deletion_operations (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
  object_path TEXT NOT NULL,
  selected_version INTEGER CHECK(selected_version > 0),
  reason TEXT NOT NULL CHECK(reason IN ('object_delete', 'retention')),
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

CREATE UNIQUE INDEX deletion_operations_logical_target
  ON deletion_operations(project_id, object_path)
  WHERE selected_version IS NULL AND reason = 'object_delete' AND status = 'pending';

CREATE UNIQUE INDEX deletion_operations_version_target
  ON deletion_operations(project_id, object_path, selected_version)
  WHERE selected_version IS NOT NULL AND reason = 'object_delete' AND status = 'pending';

CREATE UNIQUE INDEX deletion_operations_pending_retention
  ON deletion_operations(project_id)
  WHERE reason = 'retention' AND status = 'pending';

CREATE TABLE deletion_items (
  operation_id TEXT NOT NULL REFERENCES deletion_operations(id) ON DELETE CASCADE,
  version_id TEXT NOT NULL,
  storage_key TEXT NOT NULL,
  version INTEGER NOT NULL CHECK(version > 0),
  PRIMARY KEY(operation_id, version_id)
) STRICT;

CREATE TABLE audit_events (
  sequence INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
  id TEXT NOT NULL UNIQUE,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  actor TEXT NOT NULL,
  action TEXT NOT NULL,
  request_id TEXT NOT NULL,
  target_type TEXT NOT NULL,
  metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json)),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0)
) STRICT;

CREATE INDEX audit_events_workspace_sequence
  ON audit_events(workspace_id, sequence DESC);
