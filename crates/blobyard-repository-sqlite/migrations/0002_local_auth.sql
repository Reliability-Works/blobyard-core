CREATE TABLE bootstrap_authority (
  id INTEGER PRIMARY KEY NOT NULL CHECK(id = 1),
  secret_hash TEXT,
  consumed INTEGER NOT NULL CHECK(consumed IN (0, 1)),
  CHECK(
    (consumed = 0 AND secret_hash IS NOT NULL)
    OR (consumed = 1 AND secret_hash IS NULL)
  )
) STRICT;

CREATE TABLE api_tokens (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  secret_hash TEXT NOT NULL UNIQUE,
  scopes TEXT NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  revoked INTEGER NOT NULL DEFAULT 0 CHECK(revoked IN (0, 1))
) STRICT;
