CREATE TABLE cli_sessions (
  id TEXT PRIMARY KEY NOT NULL,
  token_id TEXT NOT NULL UNIQUE REFERENCES api_tokens(id) ON DELETE RESTRICT,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  name TEXT NOT NULL,
  platform TEXT NOT NULL,
  version TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  last_used_at_ms INTEGER CHECK(last_used_at_ms >= created_at_ms),
  revoked_at_ms INTEGER CHECK(revoked_at_ms >= created_at_ms)
) STRICT;

CREATE INDEX cli_sessions_workspace_active_created
  ON cli_sessions(workspace_id, revoked_at_ms, created_at_ms DESC, id DESC);
