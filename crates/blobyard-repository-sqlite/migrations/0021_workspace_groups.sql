CREATE UNIQUE INDEX local_users_id_workspace
  ON local_users(id, workspace_id);

CREATE TABLE workspace_groups (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  name TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('active', 'deactivated')),
  member_count INTEGER NOT NULL CHECK(member_count >= 0 AND member_count <= 500),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  deactivated_at_ms INTEGER,
  CHECK(
    (status = 'deactivated' AND deactivated_at_ms IS NOT NULL
      AND deactivated_at_ms >= created_at_ms)
    OR (status = 'active' AND deactivated_at_ms IS NULL)
  ),
  UNIQUE(id, workspace_id)
) STRICT;

CREATE INDEX workspace_groups_by_workspace
  ON workspace_groups(workspace_id, created_at_ms DESC, id DESC);

CREATE TABLE workspace_group_members (
  group_id TEXT NOT NULL,
  workspace_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  added_at_ms INTEGER NOT NULL CHECK(added_at_ms >= 0),
  PRIMARY KEY(group_id, user_id),
  FOREIGN KEY(group_id, workspace_id)
    REFERENCES workspace_groups(id, workspace_id) ON DELETE RESTRICT,
  FOREIGN KEY(user_id, workspace_id)
    REFERENCES local_users(id, workspace_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX workspace_group_members_by_user
  ON workspace_group_members(user_id, workspace_id, group_id);

CREATE INDEX workspace_group_members_by_group
  ON workspace_group_members(group_id, added_at_ms, user_id);

CREATE VIEW active_workspace_group_members AS
  SELECT m.group_id, m.workspace_id, m.user_id
  FROM workspace_group_members m
  JOIN workspace_groups g
    ON g.id = m.group_id
   AND g.workspace_id = m.workspace_id
   AND g.status = 'active';

CREATE INDEX yard_access_grants_by_principal_status
  ON yard_access_grants(principal_kind, principal_id, status, yard_id, environment_id);
