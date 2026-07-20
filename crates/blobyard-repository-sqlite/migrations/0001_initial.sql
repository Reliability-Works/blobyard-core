CREATE TABLE workspaces (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  slug TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE projects (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  name TEXT NOT NULL,
  slug TEXT NOT NULL,
  UNIQUE(workspace_id, slug)
) STRICT;

CREATE TABLE object_versions (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
  object_path TEXT NOT NULL,
  version INTEGER NOT NULL CHECK(version > 0),
  storage_key TEXT NOT NULL UNIQUE,
  state TEXT NOT NULL CHECK(state IN ('pending', 'complete', 'aborted')),
  size INTEGER,
  checksum TEXT,
  UNIQUE(project_id, object_path, version),
  CHECK(
    (state = 'complete' AND size IS NOT NULL AND checksum IS NOT NULL)
    OR (state != 'complete' AND size IS NULL AND checksum IS NULL)
  )
) STRICT;

CREATE INDEX object_versions_project_path
  ON object_versions(project_id, object_path, version);
