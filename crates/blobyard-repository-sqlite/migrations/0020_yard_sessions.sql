CREATE TABLE yard_continuations (
  id TEXT PRIMARY KEY NOT NULL,
  continuation_hash TEXT NOT NULL UNIQUE,
  code_hash TEXT NOT NULL UNIQUE,
  yard_id TEXT NOT NULL REFERENCES web_yards(id) ON DELETE RESTRICT,
  environment_id TEXT NOT NULL REFERENCES yard_environments(id) ON DELETE RESTRICT,
  host_label TEXT NOT NULL,
  user_id TEXT NOT NULL REFERENCES local_users(id) ON DELETE RESTRICT,
  return_path TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > created_at_ms),
  consumed_at_ms INTEGER CHECK(consumed_at_ms >= created_at_ms)
) STRICT;

CREATE INDEX yard_continuations_by_created
  ON yard_continuations(created_at_ms DESC, id DESC);

CREATE TABLE yard_sessions (
  id TEXT PRIMARY KEY NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  yard_id TEXT NOT NULL REFERENCES web_yards(id) ON DELETE RESTRICT,
  environment_id TEXT NOT NULL REFERENCES yard_environments(id) ON DELETE RESTRICT,
  host_label TEXT NOT NULL,
  user_id TEXT NOT NULL REFERENCES local_users(id) ON DELETE RESTRICT,
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > created_at_ms),
  last_used_at_ms INTEGER CHECK(last_used_at_ms >= created_at_ms),
  revoked_at_ms INTEGER CHECK(revoked_at_ms >= created_at_ms)
) STRICT;

CREATE INDEX yard_sessions_by_yard
  ON yard_sessions(yard_id, revoked_at_ms, created_at_ms DESC, id DESC);

CREATE INDEX yard_sessions_by_user
  ON yard_sessions(user_id, revoked_at_ms);
