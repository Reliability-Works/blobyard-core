CREATE UNIQUE INDEX web_yards_id_workspace
  ON web_yards(id, workspace_id);

CREATE TABLE yard_management_role_assignments (
  yard_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  workspace_id TEXT NOT NULL,
  role TEXT NOT NULL CHECK(role IN ('owner', 'admin', 'developer', 'auditor')),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
  PRIMARY KEY(yard_id, user_id),
  FOREIGN KEY(yard_id, workspace_id)
    REFERENCES web_yards(id, workspace_id) ON DELETE RESTRICT,
  FOREIGN KEY(user_id, workspace_id)
    REFERENCES local_users(id, workspace_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX yard_management_roles_by_yard_role_user
  ON yard_management_role_assignments(yard_id, role, user_id);

CREATE INDEX yard_management_roles_by_user
  ON yard_management_role_assignments(user_id, workspace_id, yard_id);

CREATE TABLE yard_application_policies (
  yard_id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL,
  revision INTEGER NOT NULL CHECK(revision > 0),
  source_manifest_digest TEXT NOT NULL CHECK(
    length(source_manifest_digest) = 64
    AND source_manifest_digest NOT GLOB '*[^0-9a-f]*'
  ),
  policy_json TEXT NOT NULL CHECK(length(policy_json) > 0),
  effective_json TEXT NOT NULL CHECK(length(effective_json) > 0),
  approved_at_ms INTEGER NOT NULL CHECK(approved_at_ms >= 0),
  approved_by_principal TEXT NOT NULL CHECK(length(approved_by_principal) > 0),
  FOREIGN KEY(yard_id, workspace_id)
    REFERENCES web_yards(id, workspace_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX yard_application_policies_by_workspace
  ON yard_application_policies(workspace_id, yard_id);
