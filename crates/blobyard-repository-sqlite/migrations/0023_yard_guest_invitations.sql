CREATE UNIQUE INDEX projects_id_workspace
  ON projects(id, workspace_id);

CREATE UNIQUE INDEX web_yards_id_project_workspace
  ON web_yards(id, project_id, workspace_id);

CREATE UNIQUE INDEX yard_environments_id_yard
  ON yard_environments(id, yard_id);

CREATE UNIQUE INDEX yard_access_grants_id_yard
  ON yard_access_grants(id, yard_id);

CREATE TABLE yard_guest_invitations (
  id TEXT PRIMARY KEY NOT NULL CHECK(
    length(id) = 36
    AND substr(id, 1, 4) = 'ygi_'
    AND substr(id, 5) NOT GLOB '*[^0-9a-f]*'
  ),
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  project_id TEXT NOT NULL,
  yard_id TEXT NOT NULL,
  environment_id TEXT,
  email TEXT NOT NULL CHECK(length(email) BETWEEN 3 AND 254),
  token_hash TEXT UNIQUE CHECK(
    token_hash IS NULL
    OR (
      length(token_hash) = 64
      AND token_hash NOT GLOB '*[^0-9a-f]*'
    )
  ),
  status TEXT NOT NULL CHECK(status IN ('pending', 'accepted', 'revoked')),
  accepted_subject_id TEXT,
  grant_id TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > created_at_ms),
  accepted_at_ms INTEGER CHECK(accepted_at_ms >= created_at_ms),
  revoked_at_ms INTEGER CHECK(revoked_at_ms >= created_at_ms),
  UNIQUE(id, workspace_id),
  FOREIGN KEY(project_id, workspace_id)
    REFERENCES projects(id, workspace_id) ON DELETE RESTRICT,
  FOREIGN KEY(yard_id, project_id, workspace_id)
    REFERENCES web_yards(id, project_id, workspace_id) ON DELETE RESTRICT,
  FOREIGN KEY(environment_id, yard_id)
    REFERENCES yard_environments(id, yard_id) ON DELETE RESTRICT,
  FOREIGN KEY(grant_id, yard_id)
    REFERENCES yard_access_grants(id, yard_id) ON DELETE RESTRICT,
  FOREIGN KEY(accepted_subject_id, workspace_id, id)
    REFERENCES yard_subjects(id, workspace_id, invitation_id) ON DELETE RESTRICT,
  CHECK(
    (status = 'pending' AND token_hash IS NOT NULL
      AND accepted_subject_id IS NULL AND accepted_at_ms IS NULL AND revoked_at_ms IS NULL)
    OR (status = 'accepted' AND token_hash IS NULL
      AND accepted_subject_id IS NOT NULL AND accepted_at_ms IS NOT NULL AND revoked_at_ms IS NULL)
    OR (status = 'revoked' AND token_hash IS NULL AND revoked_at_ms IS NOT NULL)
  )
) STRICT;

CREATE INDEX yard_guest_invitations_by_yard
  ON yard_guest_invitations(yard_id, created_at_ms DESC, id DESC);

CREATE INDEX yard_guest_invitations_by_scope_email
  ON yard_guest_invitations(yard_id, environment_id, email, status, expires_at_ms);

CREATE INDEX yard_guest_invitations_by_expiry
  ON yard_guest_invitations(status, expires_at_ms, id);

CREATE TABLE yard_subjects (
  id TEXT PRIMARY KEY NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('member', 'guest')),
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  local_user_id TEXT,
  invitation_id TEXT,
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  revoked_at_ms INTEGER CHECK(revoked_at_ms >= created_at_ms),
  UNIQUE(id, workspace_id),
  UNIQUE(id, workspace_id, invitation_id),
  UNIQUE(local_user_id),
  UNIQUE(invitation_id),
  FOREIGN KEY(local_user_id, workspace_id)
    REFERENCES local_users(id, workspace_id) ON DELETE RESTRICT,
  FOREIGN KEY(invitation_id, workspace_id)
    REFERENCES yard_guest_invitations(id, workspace_id) ON DELETE RESTRICT,
  CHECK(
    (kind = 'member' AND id = local_user_id
      AND local_user_id IS NOT NULL AND invitation_id IS NULL)
    OR (kind = 'guest' AND substr(id, 1, 6) = 'guest_'
      AND length(id) = 38 AND substr(id, 7) NOT GLOB '*[^0-9a-f]*'
      AND local_user_id IS NULL AND invitation_id IS NOT NULL)
  )
) STRICT;

CREATE INDEX yard_subjects_by_workspace
  ON yard_subjects(workspace_id, kind, id);

INSERT INTO yard_subjects (
  id, kind, workspace_id, local_user_id, invitation_id, created_at_ms, revoked_at_ms
)
SELECT
  id, 'member', workspace_id, id, NULL, created_at_ms, deactivated_at_ms
FROM local_users
ORDER BY id;

CREATE TABLE yard_guest_login_keys (
  id TEXT PRIMARY KEY NOT NULL,
  subject_id TEXT NOT NULL,
  invitation_id TEXT NOT NULL,
  workspace_id TEXT NOT NULL,
  token_prefix TEXT NOT NULL CHECK(length(token_prefix) BETWEEN 1 AND 32),
  secret_hash TEXT NOT NULL UNIQUE CHECK(
    length(secret_hash) = 64
    AND secret_hash NOT GLOB '*[^0-9a-f]*'
  ),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > created_at_ms),
  last_used_at_ms INTEGER CHECK(last_used_at_ms >= created_at_ms),
  revoked_at_ms INTEGER CHECK(revoked_at_ms >= created_at_ms),
  FOREIGN KEY(subject_id, workspace_id, invitation_id)
    REFERENCES yard_subjects(id, workspace_id, invitation_id) ON DELETE RESTRICT,
  FOREIGN KEY(invitation_id, workspace_id)
    REFERENCES yard_guest_invitations(id, workspace_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX yard_guest_login_keys_by_subject
  ON yard_guest_login_keys(subject_id, revoked_at_ms, created_at_ms DESC, id DESC);

ALTER TABLE yard_continuations RENAME TO yard_continuations_v22;

CREATE TABLE yard_continuations (
  id TEXT PRIMARY KEY NOT NULL,
  continuation_hash TEXT NOT NULL UNIQUE,
  code_hash TEXT NOT NULL UNIQUE,
  yard_id TEXT NOT NULL,
  environment_id TEXT NOT NULL,
  host_label TEXT NOT NULL,
  subject_id TEXT NOT NULL,
  return_path TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > created_at_ms),
  consumed_at_ms INTEGER CHECK(consumed_at_ms >= created_at_ms),
  FOREIGN KEY(yard_id) REFERENCES web_yards(id) ON DELETE RESTRICT,
  FOREIGN KEY(environment_id, yard_id)
    REFERENCES yard_environments(id, yard_id) ON DELETE RESTRICT,
  FOREIGN KEY(subject_id) REFERENCES yard_subjects(id) ON DELETE RESTRICT
) STRICT;

INSERT INTO yard_continuations (
  id, continuation_hash, code_hash, yard_id, environment_id, host_label,
  subject_id, return_path, created_at_ms, expires_at_ms, consumed_at_ms
)
SELECT
  c.id, c.continuation_hash, c.code_hash, c.yard_id, c.environment_id, c.host_label,
  c.user_id, c.return_path, c.created_at_ms, c.expires_at_ms, c.consumed_at_ms
FROM yard_continuations_v22 c
JOIN yard_subjects s ON s.id = c.user_id AND s.kind = 'member'
ORDER BY c.id;

DROP TABLE yard_continuations_v22;

CREATE INDEX yard_continuations_by_created
  ON yard_continuations(created_at_ms DESC, id DESC);

ALTER TABLE yard_sessions RENAME TO yard_sessions_v22;

CREATE TABLE yard_sessions (
  id TEXT PRIMARY KEY NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  yard_id TEXT NOT NULL,
  environment_id TEXT NOT NULL,
  host_label TEXT NOT NULL,
  subject_id TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > created_at_ms),
  last_used_at_ms INTEGER CHECK(last_used_at_ms >= created_at_ms),
  revoked_at_ms INTEGER CHECK(revoked_at_ms >= created_at_ms),
  FOREIGN KEY(yard_id) REFERENCES web_yards(id) ON DELETE RESTRICT,
  FOREIGN KEY(environment_id, yard_id)
    REFERENCES yard_environments(id, yard_id) ON DELETE RESTRICT,
  FOREIGN KEY(subject_id) REFERENCES yard_subjects(id) ON DELETE RESTRICT
) STRICT;

INSERT INTO yard_sessions (
  id, token_hash, yard_id, environment_id, host_label, subject_id,
  created_at_ms, expires_at_ms, last_used_at_ms, revoked_at_ms
)
SELECT
  s.id, s.token_hash, s.yard_id, s.environment_id, s.host_label, s.user_id,
  s.created_at_ms, s.expires_at_ms, s.last_used_at_ms, s.revoked_at_ms
FROM yard_sessions_v22 s
JOIN yard_subjects subject ON subject.id = s.user_id AND subject.kind = 'member'
ORDER BY s.id;

DROP TABLE yard_sessions_v22;

CREATE INDEX yard_sessions_by_yard
  ON yard_sessions(yard_id, revoked_at_ms, created_at_ms DESC, id DESC);

CREATE INDEX yard_sessions_by_subject
  ON yard_sessions(subject_id, revoked_at_ms);
