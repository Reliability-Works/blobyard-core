#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Public `SQLite` workspace-group pagination regression.

use blobyard_contract::{
    AuditValue, LocalUserRepository, MetadataRepository, RepositoryError, WorkspaceGroupCursor,
    WorkspaceGroupMemberCursor, WorkspaceGroupMemberRecord, WorkspaceGroupRecord,
    WorkspaceGroupRepository, WorkspaceGroupStatus,
};
use blobyard_repository_sqlite::SqliteRepository;

#[test]
fn sqlite_public_group_adapter_paginates_groups_and_members() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository =
        SqliteRepository::open(&temporary.path().join("groups.sqlite3")).expect("repository");
    blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
    let workspace_id = repository.list_workspaces().expect("workspaces")[0]
        .id
        .clone();
    create_users(&repository, &workspace_id);
    let groups = create_groups(&repository, &workspace_id);

    let first_page = repository
        .list_workspace_groups(&workspace_id, None, 1)
        .expect("first group page");
    let second_page = repository
        .list_workspace_groups(&workspace_id, first_page.next_cursor.as_ref(), 1)
        .expect("second group page");
    assert_eq!(first_page.items.len(), 1);
    assert_eq!(second_page.items.len(), 1);

    add_members(&repository, &groups[0]);
    let first_page = repository
        .list_workspace_group_members(&workspace_id, &groups[0].id, None, 1)
        .expect("first member page");
    let second_page = repository
        .list_workspace_group_members(
            &workspace_id,
            &groups[0].id,
            first_page.next_cursor.as_ref(),
            1,
        )
        .expect("second member page");
    assert_eq!(first_page.items.len(), 1);
    assert_eq!(second_page.items.len(), 1);
}

#[test]
fn sqlite_public_group_adapter_rejects_invalid_queries() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository =
        SqliteRepository::open(&temporary.path().join("invalid.sqlite3")).expect("repository");
    blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
    let workspace_id = repository.list_workspaces().expect("workspaces")[0]
        .id
        .clone();
    let group_cursor = WorkspaceGroupCursor {
        created_at_ms: u64::MAX,
        id: "group_00000000000000000000000000000010".to_owned(),
    };
    assert_eq!(
        repository.list_workspace_groups("", None, 1),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_workspace_groups(&workspace_id, None, 0),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_workspace_groups(
            &workspace_id,
            Some(&WorkspaceGroupCursor {
                created_at_ms: 1,
                id: "invalid".to_owned(),
            }),
            1,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_workspace_groups(&workspace_id, Some(&group_cursor), 1),
        Err(RepositoryError::InvalidInput)
    );
    let member_cursor = WorkspaceGroupMemberCursor {
        added_at_ms: u64::MAX,
        user_id: "user_page_first".to_owned(),
    };
    assert_eq!(
        repository.list_workspace_group_members("", &group_cursor.id, None, 1),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_workspace_group_members(&workspace_id, "invalid", None, 1),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_workspace_group_members(&workspace_id, &group_cursor.id, None, 0),
        Err(RepositoryError::InvalidInput)
    );
    let groups = create_groups(&repository, &workspace_id);
    assert_eq!(
        repository.list_workspace_group_members(
            &workspace_id,
            &groups[0].id,
            Some(&member_cursor),
            1,
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn sqlite_public_group_adapter_maps_missing_tables_to_unavailable() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("missing-groups.sqlite3");
    let repository = SqliteRepository::open(&path).expect("repository");
    blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
    let workspace_id = repository.list_workspaces().expect("workspaces")[0]
        .id
        .clone();
    rusqlite::Connection::open(&path)
        .expect("raw connection")
        .execute_batch("DROP TABLE workspace_groups")
        .expect("drop groups");
    assert_eq!(
        repository.list_workspace_groups(&workspace_id, None, 1),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        repository.list_workspace_group_members(
            &workspace_id,
            "group_00000000000000000000000000000010",
            None,
            1,
        ),
        Err(RepositoryError::Unavailable)
    );

    let path = temporary.path().join("missing-members.sqlite3");
    let repository = SqliteRepository::open(&path).expect("repository");
    blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
    let workspace_id = repository.list_workspaces().expect("workspaces")[0]
        .id
        .clone();
    let groups = create_groups(&repository, &workspace_id);
    rusqlite::Connection::open(&path)
        .expect("raw connection")
        .execute_batch("DROP TABLE workspace_group_members")
        .expect("drop members");
    assert_eq!(
        repository.list_workspace_group_members(&workspace_id, &groups[0].id, None, 1),
        Err(RepositoryError::Unavailable)
    );
}

fn create_users(repository: &SqliteRepository, workspace_id: &str) {
    for (index, user_id, secret, created_at_ms) in [
        (0, "user_page_first", 'a', 20),
        (1, "user_page_second", 'b', 21),
    ] {
        let user = blobyard_testkit::local_user(workspace_id, user_id, None, created_at_ms);
        repository
            .create_local_user(
                &user,
                &blobyard_testkit::login_key(
                    &format!("userkey_page_{index}"),
                    user_id,
                    secret,
                    created_at_ms,
                ),
                &blobyard_testkit::local_user_event(
                    &format!("audit_user_page_{index}"),
                    &user,
                    "user.created",
                    created_at_ms,
                ),
            )
            .expect("local user");
    }
}

fn create_groups(repository: &SqliteRepository, workspace_id: &str) -> [WorkspaceGroupRecord; 2] {
    let groups = [
        WorkspaceGroupRecord {
            id: "group_00000000000000000000000000000010".to_owned(),
            workspace_id: workspace_id.to_owned(),
            name: "First page group".to_owned(),
            status: WorkspaceGroupStatus::Active,
            member_count: 0,
            created_at_ms: 40,
            deactivated_at_ms: None,
        },
        WorkspaceGroupRecord {
            id: "group_00000000000000000000000000000011".to_owned(),
            workspace_id: workspace_id.to_owned(),
            name: "Second page group".to_owned(),
            status: WorkspaceGroupStatus::Active,
            member_count: 0,
            created_at_ms: 41,
            deactivated_at_ms: None,
        },
    ];
    for (index, group) in groups.iter().enumerate() {
        repository
            .create_workspace_group(
                group,
                &blobyard_testkit::group_event(
                    &format!("audit_group_page_{index}"),
                    "group.created",
                    group,
                    group.created_at_ms,
                    [("name", AuditValue::String(group.name.clone()))],
                ),
            )
            .expect("group");
    }
    groups
}

fn add_members(repository: &SqliteRepository, group: &WorkspaceGroupRecord) {
    for (index, user_id, added_at_ms) in [(0, "user_page_first", 50), (1, "user_page_second", 51)] {
        let member = WorkspaceGroupMemberRecord {
            group_id: group.id.clone(),
            workspace_id: group.workspace_id.clone(),
            user_id: user_id.to_owned(),
            added_at_ms,
        };
        repository
            .add_workspace_group_member(
                &member,
                &blobyard_testkit::group_event(
                    &format!("audit_group_member_page_{index}"),
                    "group.member_added",
                    group,
                    member.added_at_ms,
                    [("userId", AuditValue::String(member.user_id.clone()))],
                ),
            )
            .expect("group member");
    }
}
