#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{SqliteRepository, assert_tables};
use blobyard_contract::RepositoryError;
use rusqlite::Connection;
use std::path::Path;

#[test]
fn environment_migration_backfills_one_production_environment_per_active_yard() {
    use blobyard_contract::{MetadataRepository, WebYardRepository};

    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("metadata.sqlite3");
    let mut connection = Connection::open(&path).expect("version sixteen connection");
    super::super::migrations::apply_through(&mut connection, 16).expect("version sixteen schema");
    connection
        .execute_batch(
            "INSERT INTO workspaces (id, name, slug) VALUES ('workspace', 'Workspace', 'workspace');
             INSERT INTO projects (id, workspace_id, name, slug) VALUES ('project', 'workspace', 'Project', 'project');
             INSERT INTO web_yards VALUES ('yard_live', 'workspace', 'project', 'docs', 'docs-123456789-team', NULL, 'active', 1, 1, NULL);
             INSERT INTO web_yards VALUES ('yard_gone', 'workspace', 'project', 'gone', 'gone-123456789-team', NULL, 'deleted', 1, 2, 2);",
        )
        .expect("version sixteen fixture");
    drop(connection);

    let repository = SqliteRepository::open(&path).expect("migrated repository");
    assert_eq!(repository.schema_version().expect("schema version"), 21);
    let environments = repository
        .list_yard_environments("yard_live")
        .expect("environments");
    assert_eq!(environments.len(), 1);
    assert_eq!(environments[0].id, "yardenv_yard_live");
    assert_eq!(environments[0].name.as_str(), "production");
    assert!(
        repository
            .list_yard_environments("yard_gone")
            .expect("deleted Yard environments")
            .is_empty()
    );
    assert_tables(&repository, &["yard_environments"]);
}

#[test]
fn access_migration_adds_empty_policy_and_grant_tables() {
    use blobyard_contract::{MetadataRepository, WebYardRepository};

    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("metadata.sqlite3");
    let mut connection = Connection::open(&path).expect("version seventeen connection");
    super::super::migrations::apply_through(&mut connection, 17).expect("version seventeen schema");
    connection
        .execute_batch(
            "INSERT INTO workspaces (id, name, slug) VALUES ('workspace', 'Workspace', 'workspace');
             INSERT INTO projects (id, workspace_id, name, slug) VALUES ('project', 'workspace', 'Project', 'project');
             INSERT INTO web_yards VALUES ('yard_live', 'workspace', 'project', 'docs', 'docs-123456789-team', NULL, 'active', 1, 1, NULL);",
        )
        .expect("version seventeen fixture");
    drop(connection);

    let repository = SqliteRepository::open(&path).expect("migrated repository");
    assert_eq!(repository.schema_version().expect("schema version"), 21);
    assert!(
        repository
            .get_yard_access_policy("yard_live")
            .expect("policy")
            .is_none()
    );
    assert!(
        repository
            .list_yard_access_grants("yard_live", 1)
            .expect("grants")
            .is_empty()
    );
    assert_tables(&repository, &["yard_access_policies", "yard_access_grants"]);
}

#[test]
fn local_user_migration_adds_empty_user_and_key_tables() {
    use blobyard_contract::{LocalUserRepository, MetadataRepository};

    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("metadata.sqlite3");
    let mut connection = Connection::open(&path).expect("version eighteen connection");
    super::super::migrations::apply_through(&mut connection, 18).expect("version eighteen schema");
    connection
        .execute_batch(
            "INSERT INTO workspaces (id, name, slug) VALUES ('workspace', 'Workspace', 'workspace');",
        )
        .expect("version eighteen fixture");
    drop(connection);

    let repository = SqliteRepository::open(&path).expect("migrated repository");
    assert_eq!(repository.schema_version().expect("schema version"), 21);
    assert!(
        repository
            .list_local_users("workspace")
            .expect("users")
            .is_empty()
    );
    assert_eq!(
        repository.authenticate_local_user_key(&"ab".repeat(32), 1),
        Err(RepositoryError::NotFound)
    );
    assert_tables(&repository, &["local_users", "local_user_login_keys"]);
}

#[test]
fn yard_session_migration_adds_empty_continuation_and_session_tables() {
    use blobyard_contract::MetadataRepository;

    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("metadata.sqlite3");
    let mut connection = Connection::open(&path).expect("version nineteen connection");
    super::super::migrations::apply_through(&mut connection, 19).expect("version nineteen schema");
    drop(connection);

    let repository = SqliteRepository::open(&path).expect("migrated repository");
    assert_eq!(repository.schema_version().expect("schema version"), 21);
    let connection = repository.test_connection().expect("connection");
    for table in ["yard_continuations", "yard_sessions"] {
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("empty table");
        assert_eq!(count, 0);
    }
    drop(connection);
}

#[test]
fn group_migration_preserves_unresolved_grants_and_adds_empty_group_tables() {
    use blobyard_contract::{
        AuditValue, LifecycleRepository, MetadataRepository, WebYardRepository,
        WorkspaceGroupRecord, WorkspaceGroupRepository, WorkspaceGroupStatus,
    };

    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("metadata.sqlite3");
    seed_version_twenty_legacy_group_grant(&path);
    let repository = SqliteRepository::open(&path).expect("migrated repository");
    assert_eq!(repository.schema_version().expect("schema version"), 21);
    assert!(
        repository
            .list_workspace_groups("workspace", None, 50)
            .expect("groups")
            .items
            .is_empty()
    );
    assert_eq!(
        repository
            .list_yard_access_grants("yard_live", 2)
            .expect("legacy grants")
            .len(),
        1
    );
    let group = WorkspaceGroupRecord {
        id: "group_00000000000000000000000000000001".to_owned(),
        workspace_id: "workspace".to_owned(),
        name: "Legacy collision".to_owned(),
        status: WorkspaceGroupStatus::Active,
        member_count: 0,
        created_at_ms: 3,
        deactivated_at_ms: None,
    };
    assert_eq!(
        repository.create_workspace_group(
            &group,
            &blobyard_testkit::group_event(
                "audit_legacy_group_collision",
                "group.created",
                &group,
                3,
                [("name", AuditValue::String(group.name.clone()))],
            ),
        ),
        Err(RepositoryError::Conflict)
    );
    assert!(
        repository
            .list_audit("workspace", None, 20)
            .expect("audit")
            .items
            .is_empty()
    );
    assert_tables(
        &repository,
        &["workspace_groups", "workspace_group_members"],
    );
}

fn seed_version_twenty_legacy_group_grant(path: &Path) {
    let mut connection = Connection::open(path).expect("version twenty connection");
    super::super::migrations::apply_through(&mut connection, 20).expect("version twenty schema");
    connection
        .execute_batch(
            "INSERT INTO workspaces (id, name, slug) VALUES ('workspace', 'Workspace', 'workspace');
             INSERT INTO projects (id, workspace_id, name, slug) VALUES ('project', 'workspace', 'Project', 'project');
             INSERT INTO web_yards VALUES ('yard_live', 'workspace', 'project', 'docs', 'docs-123456789-team', NULL, 'active', 1, 1, NULL);
             INSERT INTO yard_access_grants VALUES (
               'grant_legacy_group', 'yard_live', NULL, 'group',
               'group_00000000000000000000000000000001', '[\"viewer\"]',
               'active', 2, 'fixture', NULL, NULL
             );",
        )
        .expect("version twenty fixture");
}

#[test]
fn partial_migration_rejects_newer_targets_and_maps_each_database_failure() {
    assert_eq!(
        super::super::migrations::apply_through(
            &mut Connection::open_in_memory().expect("newer connection"),
            super::super::migrations::CURRENT_SCHEMA_VERSION + 1,
        ),
        Err(RepositoryError::SchemaTooNew)
    );

    let completed = (0..1_000).find(|&denied_index| {
        let mut connection = Connection::open_in_memory().expect("denied connection");
        let observed = super::install_denial(&connection, denied_index);
        let result = super::super::migrations::apply_through(&mut connection, 9);
        let count = observed.load(std::sync::atomic::Ordering::Relaxed);
        if count <= denied_index {
            result.expect("migration succeeds after every authorization point");
            true
        } else {
            assert_eq!(result, Err(RepositoryError::Unavailable));
            false
        }
    });
    assert!(completed.is_some(), "migration denial sweep must terminate");
}
