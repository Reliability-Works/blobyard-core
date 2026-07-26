CREATE TABLE yard_access_policies (
  yard_id TEXT PRIMARY KEY NOT NULL REFERENCES web_yards(id) ON DELETE RESTRICT,
  visibility TEXT NOT NULL CHECK(
    visibility IN ('public', 'owner', 'selected', 'workspace', 'authenticated-link', 'any-authenticated')
  ),
  updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
  updated_by_principal TEXT NOT NULL
) STRICT;

CREATE TABLE yard_access_grants (
  id TEXT PRIMARY KEY NOT NULL,
  yard_id TEXT NOT NULL REFERENCES web_yards(id) ON DELETE RESTRICT,
  environment_id TEXT REFERENCES yard_environments(id) ON DELETE RESTRICT,
  principal_kind TEXT NOT NULL CHECK(principal_kind IN ('user', 'group', 'guest-invite', 'link')),
  principal_id TEXT NOT NULL,
  app_roles TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('active', 'revoked')),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  created_by_principal TEXT NOT NULL,
  expires_at_ms INTEGER CHECK(expires_at_ms IS NULL OR expires_at_ms >= created_at_ms),
  revoked_at_ms INTEGER,
  CHECK(
    (status = 'revoked' AND revoked_at_ms IS NOT NULL AND revoked_at_ms >= created_at_ms)
    OR (status != 'revoked' AND revoked_at_ms IS NULL)
  )
) STRICT;

CREATE INDEX yard_access_grants_by_yard
  ON yard_access_grants(yard_id, created_at_ms DESC, id DESC);

CREATE INDEX yard_access_grants_by_principal
  ON yard_access_grants(yard_id, principal_kind, principal_id);
