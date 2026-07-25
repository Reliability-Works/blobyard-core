#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Exact upper-bound and insert-between-pages group pagination coverage.

use blobyard_contract::{MetadataRepository, WorkspaceGroupRepository};
use blobyard_repository_sqlite::SqliteRepository;
use rusqlite::Connection;
use std::collections::BTreeSet;

#[test]
fn exact_group_pagination_suite_executes_every_generated_case() {
    let mut tracker = blobyard_testkit::FixtureExecutionTracker::new("sqlite", "group-pagination");
    fifty_item_group_page_keeps_its_cursor_stable_across_an_insert(&mut tracker);
    fifty_item_member_page_keeps_its_cursor_stable_across_an_insert(&mut tracker);
    tracker.finish().expect("complete pagination fixtures");
}

fn fifty_item_group_page_keeps_its_cursor_stable_across_an_insert(
    tracker: &mut blobyard_testkit::FixtureExecutionTracker,
) {
    let fixture = Fixture::new("groups-exact.sqlite3");
    seed_groups(&fixture.path);
    let first = fixture
        .repository
        .list_workspace_groups("workspace_fixture", None, 50)
        .expect("first page");
    assert_eq!(first.items.len(), 50);
    assert_eq!(first.items.first().expect("newest").created_at_ms, 1_051);
    insert_new_group(&fixture.path);
    let second = fixture
        .repository
        .list_workspace_groups("workspace_fixture", first.next_cursor.as_ref(), 50)
        .expect("second page");
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].created_at_ms, 1_001);
    assert!(second.next_cursor.is_none());
    let snapshot_ids = first
        .items
        .iter()
        .chain(&second.items)
        .map(|group| group.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(snapshot_ids.len(), 51);
    let refreshed = fixture
        .repository
        .list_workspace_groups("workspace_fixture", None, 50)
        .expect("refreshed page");
    assert_eq!(
        refreshed.items[0].id,
        "group_ffffffffffffffffffffffffffffffff"
    );
    tracker.record_case(
        "group-pagination-is-deterministic-and-cursor-safe",
        &serde_json::json!({
            "resource": "groups",
            "pageSize": 50,
            "concurrentInsertAfterCursor": true
        }),
        &serde_json::json!({
            "ordering": ["createdAt-desc", "id-desc"],
            "duplicates": 0,
            "omissionsWithinSnapshot": 0
        }),
    );
}

fn fifty_item_member_page_keeps_its_cursor_stable_across_an_insert(
    tracker: &mut blobyard_testkit::FixtureExecutionTracker,
) {
    let fixture = Fixture::new("members-exact.sqlite3");
    seed_members(&fixture.path);
    let first = fixture
        .repository
        .list_workspace_group_members("workspace_fixture", target_group(), None, 50)
        .expect("first page");
    assert_eq!(first.items.len(), 50);
    assert_eq!(first.items.first().expect("newest").added_at_ms, 1_051);
    insert_new_member(&fixture.path);
    let second = fixture
        .repository
        .list_workspace_group_members(
            "workspace_fixture",
            target_group(),
            first.next_cursor.as_ref(),
            50,
        )
        .expect("second page");
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].added_at_ms, 1_001);
    assert!(second.next_cursor.is_none());
    let snapshot_ids = first
        .items
        .iter()
        .chain(&second.items)
        .map(|member| member.user_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(snapshot_ids.len(), 51);
    let refreshed = fixture
        .repository
        .list_workspace_group_members("workspace_fixture", target_group(), None, 50)
        .expect("refreshed page");
    assert_eq!(refreshed.items[0].user_id, "user_page_new");
    tracker.record_case(
        "member-pagination-is-deterministic-and-cursor-safe",
        &serde_json::json!({
            "resource": "group-members",
            "pageSize": 50,
            "concurrentInsertAfterCursor": true
        }),
        &serde_json::json!({
            "ordering": ["addedAt-desc", "userId-desc"],
            "duplicates": 0,
            "omissionsWithinSnapshot": 0
        }),
    );
}

struct Fixture {
    _temporary: tempfile::TempDir,
    path: std::path::PathBuf,
    repository: SqliteRepository,
}

impl Fixture {
    fn new(database: &str) -> Self {
        let temporary = tempfile::tempdir().expect("temporary");
        let path = temporary.path().join(database);
        let repository = SqliteRepository::open(&path).expect("repository");
        blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
        assert_eq!(
            repository.list_workspaces().expect("workspaces")[0].id,
            "workspace_fixture"
        );
        Self {
            _temporary: temporary,
            path,
            repository,
        }
    }
}

const fn target_group() -> &'static str {
    "group_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
}

fn seed_groups(path: &std::path::Path) {
    Connection::open(path)
        .expect("connection")
        .execute_batch(
            "WITH RECURSIVE values_to_insert(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM values_to_insert WHERE value < 51
             )
             INSERT INTO workspace_groups
               (id, workspace_id, name, status, member_count, created_at_ms)
             SELECT printf('group_%032x', value), 'workspace_fixture',
                    printf('Group %d', value), 'active', 0, value + 1000
             FROM values_to_insert;",
        )
        .expect("groups");
}

fn insert_new_group(path: &std::path::Path) {
    Connection::open(path)
        .expect("connection")
        .execute(
            "INSERT INTO workspace_groups
               (id, workspace_id, name, status, member_count, created_at_ms)
             VALUES (?1, 'workspace_fixture', 'Concurrent group', 'active', 0, 2000)",
            [target_new_group()],
        )
        .expect("new group");
}

const fn target_new_group() -> &'static str {
    "group_ffffffffffffffffffffffffffffffff"
}

fn seed_members(path: &std::path::Path) {
    Connection::open(path)
        .expect("connection")
        .execute_batch(&format!(
            "INSERT INTO workspace_groups
               (id, workspace_id, name, status, member_count, created_at_ms)
             VALUES ('{}', 'workspace_fixture', 'Members', 'active', 51, 1000);
             WITH RECURSIVE values_to_insert(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM values_to_insert WHERE value < 51
             )
             INSERT INTO local_users
               (id, workspace_id, display_name, status, created_at_ms)
             SELECT printf('user_page_%03d', value), 'workspace_fixture',
                    printf('User %d', value), 'active', value + 1000
             FROM values_to_insert;
             WITH RECURSIVE values_to_insert(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM values_to_insert WHERE value < 51
             )
             INSERT INTO workspace_group_members
               (group_id, workspace_id, user_id, added_at_ms)
             SELECT '{}', 'workspace_fixture', printf('user_page_%03d', value), value + 1000
             FROM values_to_insert;",
            target_group(),
            target_group(),
        ))
        .expect("members");
}

fn insert_new_member(path: &std::path::Path) {
    Connection::open(path)
        .expect("connection")
        .execute_batch(&format!(
            "INSERT INTO local_users
               (id, workspace_id, display_name, status, created_at_ms)
             VALUES ('user_page_new', 'workspace_fixture', 'Concurrent user', 'active', 2000);
             INSERT INTO workspace_group_members
               (group_id, workspace_id, user_id, added_at_ms)
             VALUES ('{}', 'workspace_fixture', 'user_page_new', 2000);
             UPDATE workspace_groups SET member_count = 52 WHERE id = '{}';",
            target_group(),
            target_group(),
        ))
        .expect("new member");
}
