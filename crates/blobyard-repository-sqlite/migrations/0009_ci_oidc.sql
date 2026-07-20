CREATE TABLE ci_trusts (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  project_id TEXT REFERENCES projects(id) ON DELETE RESTRICT,
  repository TEXT NOT NULL,
  workflow_path TEXT NOT NULL,
  workflow_ref TEXT NOT NULL,
  allowed_ref_glob TEXT NOT NULL,
  environment TEXT,
  allowed_actions TEXT NOT NULL,
  audience TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  revoked_at_ms INTEGER CHECK(revoked_at_ms >= created_at_ms)
) STRICT;

CREATE INDEX ci_trusts_workspace_created
  ON ci_trusts(workspace_id, created_at_ms DESC, id DESC);

CREATE INDEX ci_trusts_repository_created
  ON ci_trusts(repository, created_at_ms DESC, id DESC);

CREATE TABLE machine_sessions (
  id TEXT PRIMARY KEY NOT NULL,
  token_id TEXT NOT NULL UNIQUE REFERENCES api_tokens(id) ON DELETE RESTRICT,
  trust_id TEXT NOT NULL REFERENCES ci_trusts(id) ON DELETE RESTRICT,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
  repository TEXT NOT NULL,
  git_ref TEXT NOT NULL,
  run_id TEXT NOT NULL,
  run_attempt TEXT,
  actions TEXT NOT NULL,
  oidc_token_hash TEXT NOT NULL UNIQUE,
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > created_at_ms),
  last_used_at_ms INTEGER CHECK(last_used_at_ms >= created_at_ms),
  revoked_at_ms INTEGER CHECK(revoked_at_ms >= created_at_ms)
) STRICT;

CREATE INDEX machine_sessions_trust_created
  ON machine_sessions(trust_id, created_at_ms DESC, id DESC);
