#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Relational corruption coverage for group tenant, membership, and grant rows.

use blobyard_contract::{WebYardRepository, WorkspaceGroupRepository};
use blobyard_repository_sqlite::SqliteRepository;
use rusqlite::Connection;

#[test]
fn corrupt_group_tenant_and_membership_rows_are_concealed() {
    let fixture = Fixture::new();
    corrupt_group_relations(&fixture.path);
    assert!(
        fixture
            .repository
            .list_workspace_groups("workspace_ghost", None, 50)
            .expect("ghost groups")
            .items
            .is_empty()
    );
    assert!(
        fixture
            .repository
            .list_workspace_group_members(
                "workspace_fixture",
                "group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                None,
                50,
            )
            .expect("corrupt members")
            .items
            .is_empty()
    );
}

#[test]
fn corrupt_grant_rows_fail_closed() {
    let fixture = Fixture::new();
    corrupt_grant(&fixture.path);
    assert_eq!(
        fixture
            .repository
            .list_yard_access_grants("yard_corrupt", 10)
            .err(),
        Some(blobyard_contract::RepositoryError::Unavailable)
    );
}

struct Fixture {
    _temporary: tempfile::TempDir,
    path: std::path::PathBuf,
    repository: SqliteRepository,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary");
        let path = temporary.path().join("corruption.sqlite3");
        let repository = SqliteRepository::open(&path).expect("repository");
        blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
        Self {
            _temporary: temporary,
            path,
            repository,
        }
    }
}

fn corrupt_group_relations(path: &std::path::Path) {
    Connection::open(path)
        .expect("connection")
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             INSERT INTO workspace_groups
               (id, workspace_id, name, status, member_count, created_at_ms)
             VALUES
               ('group_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'workspace_ghost',
                'Ghost group', 'active', 0, 1),
               ('group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'workspace_fixture',
                'Fixture group', 'active', 1, 1);
             INSERT INTO workspaces VALUES ('workspace_foreign', 'Foreign', 'foreign');
             INSERT INTO local_users
               (id, workspace_id, display_name, status, created_at_ms)
             VALUES ('user_foreign', 'workspace_foreign', 'Foreign user', 'active', 1);
             INSERT INTO workspace_group_members VALUES
               ('group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'workspace_fixture',
                'user_foreign', 2);",
        )
        .expect("corrupt group relations");
}

fn corrupt_grant(path: &std::path::Path) {
    Connection::open(path)
        .expect("connection")
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             PRAGMA ignore_check_constraints = ON;
             INSERT INTO web_yards
               (id, workspace_id, project_id, name, host_label, status, created_at_ms, updated_at_ms)
             VALUES
               ('yard_corrupt', 'workspace_fixture', 'project_fixture', 'corrupt',
                'corrupt-fixture', 'active', 1, 1);
             INSERT INTO yard_access_grants
               (id, yard_id, principal_kind, principal_id, app_roles, status,
                created_at_ms, created_by_principal)
             VALUES
               ('grant_corrupt', 'yard_corrupt', 'group', '', '[]', 'active', 1, 'fixture');",
        )
        .expect("corrupt grant");
}
