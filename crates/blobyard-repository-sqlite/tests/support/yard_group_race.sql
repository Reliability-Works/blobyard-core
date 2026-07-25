INSERT INTO workspaces VALUES ('workspace_fixture', 'Fixture', 'fixture');
INSERT INTO projects VALUES
  ('project_fixture', 'workspace_fixture', 'Fixture project', 'fixture-project');
INSERT INTO object_versions
  (id, project_id, object_path, version, storage_key, state, size, checksum)
VALUES
  ('version_fixture', 'project_fixture', 'asset.js', 1, 'objects/version_fixture',
   'complete', 1, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
INSERT INTO upload_reservations
  (id, version_id, filename, content_type, expected_size, expected_checksum,
   capability_hash, expires_at_ms, state, received_size, received_checksum)
VALUES
  ('upload_fixture', 'version_fixture', 'asset.js', 'text/javascript', 1,
   'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
   'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
   1000000, 'complete', 1,
   'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
INSERT INTO local_users VALUES
  ('user_fixture', 'workspace_fixture', 'Fixture user', NULL, 'active', 1, NULL);
INSERT INTO web_yards VALUES
  ('yard_fixture', 'workspace_fixture', 'project_fixture', 'docs', 'docs-fixture',
   'deploy_fixture', 'active', 1, 1, NULL);
INSERT INTO yard_deploys VALUES
  ('deploy_fixture', 'yard_fixture', 'workspace_fixture', 'project_fixture',
   'clientdeploy00000001', '.blobyard-yard/yard_fixture/clientdeploy00000001/',
   'docs-deploy-fixture', 0, 0, 'live', 1, 2, 1, 1, NULL, NULL, NULL);
INSERT INTO yard_deploy_files VALUES
  ('deploy_fixture', 'asset.js', 'version_fixture', 1);
INSERT INTO yard_environments VALUES
  ('environment_fixture', 'yard_fixture', 'production', 'production', 'active',
   1, 1, NULL);
INSERT INTO yard_access_policies VALUES
  ('yard_fixture', 'selected', 1, 'fixture');
INSERT INTO workspace_groups
  (id, workspace_id, name, status, member_count, created_at_ms)
VALUES
  ('group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'workspace_fixture',
   'Readers', 'active', 1, 2);
INSERT INTO workspace_group_members VALUES
  ('group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'workspace_fixture',
   'user_fixture', 2);
INSERT INTO yard_access_grants VALUES
  ('grant_fixture', 'yard_fixture', NULL, 'group',
   'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '[]',
   'active', 2, 'fixture', NULL, NULL);
