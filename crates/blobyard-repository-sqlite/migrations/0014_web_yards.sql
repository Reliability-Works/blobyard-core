CREATE TABLE web_yards (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
  name TEXT NOT NULL,
  host_label TEXT NOT NULL UNIQUE,
  current_deploy_id TEXT,
  status TEXT NOT NULL CHECK(status IN ('active', 'suspended', 'deleted')),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
  deleted_at_ms INTEGER,
  CHECK(
    (status = 'deleted' AND current_deploy_id IS NULL AND deleted_at_ms IS NOT NULL AND deleted_at_ms >= created_at_ms)
    OR (status != 'deleted' AND deleted_at_ms IS NULL)
  )
) STRICT;

CREATE UNIQUE INDEX web_yards_active_project_name
  ON web_yards(project_id, name)
  WHERE status != 'deleted';

CREATE INDEX web_yards_project_created
  ON web_yards(project_id, created_at_ms DESC, id DESC);

CREATE TABLE yard_deploys (
  id TEXT PRIMARY KEY NOT NULL,
  yard_id TEXT NOT NULL REFERENCES web_yards(id) ON DELETE RESTRICT,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
  client_deploy_id TEXT NOT NULL UNIQUE,
  manifest_root TEXT NOT NULL UNIQUE,
  deployment_host_label TEXT NOT NULL UNIQUE,
  spa INTEGER NOT NULL CHECK(spa IN (0, 1)),
  clean_urls INTEGER NOT NULL CHECK(clean_urls IN (0, 1)),
  status TEXT NOT NULL CHECK(status IN ('uploading', 'finalising', 'live', 'failed', 'superseded', 'pruned')),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  finalised_at_ms INTEGER,
  file_count INTEGER NOT NULL DEFAULT 0 CHECK(file_count >= 0),
  total_bytes INTEGER NOT NULL DEFAULT 0 CHECK(total_bytes >= 0),
  failure_code TEXT,
  failure_message TEXT,
  pruned_at_ms INTEGER,
  CHECK(finalised_at_ms IS NULL OR finalised_at_ms >= created_at_ms),
  CHECK(pruned_at_ms IS NULL OR pruned_at_ms >= created_at_ms),
  CHECK(
    (status IN ('uploading', 'finalising') AND finalised_at_ms IS NULL AND file_count = 0 AND total_bytes = 0 AND failure_code IS NULL AND failure_message IS NULL AND pruned_at_ms IS NULL)
    OR (status IN ('live', 'superseded') AND finalised_at_ms IS NOT NULL AND file_count > 0 AND failure_code IS NULL AND failure_message IS NULL AND pruned_at_ms IS NULL)
    OR (status = 'failed' AND finalised_at_ms IS NULL AND file_count = 0 AND total_bytes = 0 AND failure_code IS NOT NULL AND failure_message IS NOT NULL AND pruned_at_ms IS NULL)
    OR (status = 'pruned' AND pruned_at_ms IS NOT NULL)
  )
) STRICT;

CREATE INDEX yard_deploys_yard_created
  ON yard_deploys(yard_id, created_at_ms DESC, id DESC);

CREATE TABLE yard_deploy_files (
  deploy_id TEXT NOT NULL REFERENCES yard_deploys(id) ON DELETE CASCADE,
  normalized_path TEXT NOT NULL,
  version_id TEXT REFERENCES object_versions(id) ON DELETE SET NULL,
  byte_size INTEGER NOT NULL CHECK(byte_size >= 0),
  PRIMARY KEY(deploy_id, normalized_path)
) STRICT;

CREATE INDEX yard_deploy_files_version
  ON yard_deploy_files(version_id);
