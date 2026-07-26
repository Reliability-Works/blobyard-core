#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::SqliteRepository;
use blobyard_contract::{
    AuditValue, NewYardAccessGrant, RepositoryError, WebYardRepository, WorkspaceGroupMemberRecord,
    WorkspaceGroupRecord, WorkspaceGroupRepository, WorkspaceGroupStatus, YardAccessPrincipalKind,
};
use std::sync::atomic::Ordering;

#[path = "adapter_group_fault_support.rs"]
mod support;
use support::{audit_exists, group_count, group_count_and_members, group_state, seed_yard};

const GROUP_ID: &str = "group_00000000000000000000000000000050";
const USER_ID: &str = "user_group";
const GRANT_ID: &str = "grant_group_fault";

#[test]
fn injected_faults_roll_back_every_group_mutation_and_audit() {
    sweep(|_, _| {}, create_operation, verify_create_rollback);
    sweep(seed_group, rename_operation, verify_rename_rollback);
    sweep(seed_group, add_operation, verify_add_rollback);
    sweep(seed_group_member, remove_operation, verify_remove_rollback);
    sweep(
        seed_group_member_and_grant,
        deactivate_operation,
        verify_deactivate_rollback,
    );
}

fn sweep(
    setup: impl Fn(&SqliteRepository, &WorkspaceGroupRecord),
    operation: impl Fn(&SqliteRepository, &WorkspaceGroupRecord) -> Result<(), RepositoryError>,
    verify: impl Fn(&SqliteRepository, &WorkspaceGroupRecord),
) {
    let probe = fault_repository();
    let group = group();
    setup(&probe, &group);
    let observed = {
        let connection = probe.test_connection().expect("connection");
        super::install_denial(&connection, usize::MAX)
    };
    operation(&probe, &group).expect("operation succeeds without an injected denial");
    let write_points = observed.load(Ordering::Relaxed);
    assert!(
        write_points > 0,
        "mutation must have write authorization points"
    );

    let repository = fault_repository();
    setup(&repository, &group);
    for denied_index in 0..write_points {
        let observed = {
            let connection = repository.test_connection().expect("connection");
            super::install_denial(&connection, denied_index)
        };
        let result = operation(&repository, &group);
        assert!(observed.load(Ordering::Relaxed) > denied_index);
        assert_eq!(result, Err(RepositoryError::Unavailable));
        verify(&repository, &group);
    }
}

fn fault_repository() -> SqliteRepository {
    let connection = rusqlite::Connection::open_in_memory().expect("connection");
    let repository = SqliteRepository::initialize_connection(connection).expect("repository");
    repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "INSERT INTO workspaces VALUES ('workspace_fixture', 'Fixture', 'fixture');
             INSERT INTO projects VALUES
               ('project_fixture', 'workspace_fixture', 'Fixture project', 'fixture-project');
             INSERT INTO local_users VALUES
               ('user_group', 'workspace_fixture', 'Group user', NULL, 'active', 30, NULL);",
        )
        .expect("fault fixture");
    repository
}

fn group() -> WorkspaceGroupRecord {
    WorkspaceGroupRecord {
        id: GROUP_ID.to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        name: "Fault group".to_owned(),
        status: WorkspaceGroupStatus::Active,
        member_count: 0,
        created_at_ms: 40,
        deactivated_at_ms: None,
    }
}

fn seed_group(repository: &SqliteRepository, group: &WorkspaceGroupRecord) {
    repository
        .create_workspace_group(group, &create_event(group))
        .expect("group");
}

fn seed_group_member(repository: &SqliteRepository, group: &WorkspaceGroupRecord) {
    seed_group(repository, group);
    repository
        .add_workspace_group_member(
            &member(group),
            &member_event(group, "audit_setup_member", "group.member_added"),
        )
        .expect("member");
}

fn seed_group_member_and_grant(repository: &SqliteRepository, group: &WorkspaceGroupRecord) {
    seed_group_member(repository, group);
    let yard = seed_yard(repository);
    let grant = NewYardAccessGrant {
        id: GRANT_ID.to_owned(),
        yard_id: yard.id.clone(),
        environment_id: None,
        principal_kind: YardAccessPrincipalKind::Group,
        principal_id: group.id.clone(),
        app_roles: Vec::new(),
        created_at_ms: 42,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms: None,
    };
    repository
        .insert_yard_access_grant(
            &grant,
            &blobyard_testkit::granted_event(&yard.id, &grant, 42),
        )
        .expect("grant");
}

fn create_operation(
    repository: &SqliteRepository,
    group: &WorkspaceGroupRecord,
) -> Result<(), RepositoryError> {
    repository.create_workspace_group(group, &create_event(group))
}

fn rename_operation(
    repository: &SqliteRepository,
    group: &WorkspaceGroupRecord,
) -> Result<(), RepositoryError> {
    repository
        .rename_workspace_group(
            &group.workspace_id,
            &group.id,
            "Renamed fault group",
            &blobyard_testkit::group_event(
                "audit_fault_rename",
                "group.renamed",
                group,
                41,
                [("to", AuditValue::String("Renamed fault group".to_owned()))],
            ),
        )
        .map(|_| ())
}

fn add_operation(
    repository: &SqliteRepository,
    group: &WorkspaceGroupRecord,
) -> Result<(), RepositoryError> {
    repository.add_workspace_group_member(
        &member(group),
        &member_event(group, "audit_fault_add", "group.member_added"),
    )
}

fn remove_operation(
    repository: &SqliteRepository,
    group: &WorkspaceGroupRecord,
) -> Result<(), RepositoryError> {
    repository.remove_workspace_group_member(
        &group.workspace_id,
        &group.id,
        USER_ID,
        &member_event(group, "audit_fault_remove", "group.member_removed"),
    )
}

fn deactivate_operation(
    repository: &SqliteRepository,
    group: &WorkspaceGroupRecord,
) -> Result<(), RepositoryError> {
    repository.deactivate_workspace_group(
        &group.workspace_id,
        &group.id,
        43,
        &blobyard_testkit::group_event(
            "audit_fault_deactivate",
            "group.deactivated",
            group,
            43,
            [],
        ),
    )
}

fn verify_create_rollback(repository: &SqliteRepository, group: &WorkspaceGroupRecord) {
    assert_eq!(group_count(repository, &group.id), 0);
    assert!(!audit_exists(repository, "audit_fault_create"));
}

fn verify_rename_rollback(repository: &SqliteRepository, group: &WorkspaceGroupRecord) {
    assert_eq!(
        group_state(repository, &group.id),
        ("Fault group".to_owned(), "active".to_owned(), 0, None)
    );
    assert!(!audit_exists(repository, "audit_fault_rename"));
}

fn verify_add_rollback(repository: &SqliteRepository, group: &WorkspaceGroupRecord) {
    assert_eq!(group_count_and_members(repository, &group.id), (0, 0));
    assert!(!audit_exists(repository, "audit_fault_add"));
}

fn verify_remove_rollback(repository: &SqliteRepository, group: &WorkspaceGroupRecord) {
    assert_eq!(group_count_and_members(repository, &group.id), (1, 1));
    assert!(!audit_exists(repository, "audit_fault_remove"));
}

fn verify_deactivate_rollback(repository: &SqliteRepository, group: &WorkspaceGroupRecord) {
    assert_eq!(
        group_state(repository, &group.id),
        ("Fault group".to_owned(), "active".to_owned(), 1, None)
    );
    assert_eq!(group_count_and_members(repository, &group.id), (1, 1));
    let connection = repository.test_connection().expect("connection");
    let grant: (String, Option<i64>) = connection
        .query_row(
            "SELECT status, revoked_at_ms FROM yard_access_grants WHERE id = ?1",
            [GRANT_ID],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("grant state");
    assert_eq!(grant, ("active".to_owned(), None));
    drop(connection);
    assert!(!audit_exists(repository, "audit_fault_deactivate"));
}

fn create_event(group: &WorkspaceGroupRecord) -> blobyard_contract::NewAuditEvent {
    blobyard_testkit::group_event(
        "audit_fault_create",
        "group.created",
        group,
        40,
        [("name", AuditValue::String(group.name.clone()))],
    )
}

fn member(group: &WorkspaceGroupRecord) -> WorkspaceGroupMemberRecord {
    WorkspaceGroupMemberRecord {
        group_id: group.id.clone(),
        workspace_id: group.workspace_id.clone(),
        user_id: USER_ID.to_owned(),
        added_at_ms: 41,
    }
}

fn member_event(
    group: &WorkspaceGroupRecord,
    id: &str,
    action: &str,
) -> blobyard_contract::NewAuditEvent {
    blobyard_testkit::group_event(
        id,
        action,
        group,
        41,
        [("userId", AuditValue::String(USER_ID.to_owned()))],
    )
}
