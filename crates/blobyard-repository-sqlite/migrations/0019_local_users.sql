CREATE TABLE local_users (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  display_name TEXT NOT NULL,
  email TEXT,
  status TEXT NOT NULL CHECK(status IN ('active', 'deactivated')),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  deactivated_at_ms INTEGER,
  CHECK(
    (status = 'deactivated' AND deactivated_at_ms IS NOT NULL
      AND deactivated_at_ms >= created_at_ms)
    OR (status != 'deactivated' AND deactivated_at_ms IS NULL)
  )
) STRICT;

CREATE INDEX local_users_by_workspace
  ON local_users(workspace_id, created_at_ms DESC, id DESC);

CREATE UNIQUE INDEX local_users_workspace_email
  ON local_users(workspace_id, email)
  WHERE email IS NOT NULL AND status != 'deactivated';

CREATE TABLE local_user_login_keys (
  id TEXT PRIMARY KEY NOT NULL,
  user_id TEXT NOT NULL REFERENCES local_users(id) ON DELETE RESTRICT,
  token_prefix TEXT NOT NULL,
  secret_hash TEXT NOT NULL UNIQUE,
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms >= 0),
  last_used_at_ms INTEGER CHECK(last_used_at_ms >= created_at_ms),
  revoked_at_ms INTEGER CHECK(revoked_at_ms >= created_at_ms)
) STRICT;

CREATE INDEX local_user_login_keys_by_user
  ON local_user_login_keys(user_id, revoked_at_ms, created_at_ms DESC, id DESC);
