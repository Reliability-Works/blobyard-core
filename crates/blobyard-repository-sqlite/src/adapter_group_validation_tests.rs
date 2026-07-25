#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
#![allow(
    clippy::too_many_lines,
    reason = "group boundary tests keep each mutation matrix in one visible assertion flow"
)]

use blobyard_contract::{
    AuditValue, LocalUserRepository, RepositoryError, WorkspaceGroupCursor,
    WorkspaceGroupMemberRecord, WorkspaceGroupRecord, WorkspaceGroupRepository,
    WorkspaceGroupStatus,
};

const GROUP_ID: &str = "group_00000000000000000000000000000031";

fn group() -> WorkspaceGroupRecord {
    WorkspaceGroupRecord {
        id: GROUP_ID.to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        name: "Reviewers".to_owned(),
        status: WorkspaceGroupStatus::Active,
        member_count: 0,
        created_at_ms: 40,
        deactivated_at_ms: None,
    }
}

fn repository() -> (tempfile::TempDir, super::SqliteRepository) {
    let (temporary, repository) = super::empty_repository();
    blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
    let user = blobyard_testkit::local_user("workspace_fixture", "user_group", None, 30);
    repository
        .create_local_user(
            &user,
            &blobyard_testkit::login_key("userkey_group_validation", &user.id, 'a', 30),
            &blobyard_testkit::local_user_event(
                "audit_user_group_validation",
                &user,
                "user.created",
                30,
            ),
        )
        .expect("group user");
    let group = group();
    repository
        .create_workspace_group(
            &group,
            &blobyard_testkit::group_event(
                "audit_group_validation",
                "group.created",
                &group,
                40,
                [("name", AuditValue::String(group.name.clone()))],
            ),
        )
        .expect("group");
    (temporary, repository)
}

fn event(
    id: &str,
    action: &str,
    group: &WorkspaceGroupRecord,
    at: u64,
    user_id: Option<&str>,
) -> blobyard_contract::NewAuditEvent {
    let mut event = blobyard_testkit::group_event(id, action, group, at, []);
    if let Some(user_id) = user_id {
        event
            .metadata
            .push(("userId".to_owned(), AuditValue::String(user_id.to_owned())));
    }
    event
}

#[test]
fn group_and_member_validation_reject_every_invalid_identity_field() {
    let mut record = group();
    record.id = "group_invalid".to_owned();
    assert_eq!(
        super::super::workspace_group_rows::validate_group(&record),
        Err(RepositoryError::InvalidInput)
    );
    record = group();
    record.workspace_id.clear();
    assert_eq!(
        super::super::workspace_group_rows::validate_group(&record),
        Err(RepositoryError::InvalidInput)
    );
    record = group();
    record.name = "x".to_owned();
    assert_eq!(
        super::super::workspace_group_rows::validate_group(&record),
        Err(RepositoryError::InvalidInput)
    );

    for member in [
        member("group_invalid", "workspace_fixture", "user_group", 41),
        member(GROUP_ID, "", "user_group", 41),
        member(GROUP_ID, "workspace_fixture", "", 41),
    ] {
        assert_eq!(
            super::super::workspace_group_rows::validate_member(&member),
            Err(RepositoryError::InvalidInput)
        );
    }
}

#[test]
fn public_group_boundaries_reject_invalid_inputs_before_mutation() {
    let (_temporary, repository) = repository();
    let group = group();
    let created = event("audit_invalid_boundary", "group.created", &group, 40, None);
    let mut invalid_group = group;
    invalid_group.id = "group_invalid".to_owned();
    assert_eq!(
        repository.create_workspace_group(
            &invalid_group,
            &event(
                "audit_invalid_create_boundary",
                "group.created",
                &invalid_group,
                40,
                None,
            ),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_workspace_groups("", None, 50),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_workspace_groups(
            "workspace_fixture",
            Some(&WorkspaceGroupCursor {
                created_at_ms: 40,
                id: "group_invalid".to_owned(),
            }),
            50,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_workspace_group_members("", GROUP_ID, None, 50),
        Err(RepositoryError::InvalidInput)
    );
    for (workspace, group_id, name) in [
        ("", GROUP_ID, "Reviewers"),
        ("workspace_fixture", "group_invalid", "Reviewers"),
        ("workspace_fixture", GROUP_ID, "x"),
    ] {
        assert_eq!(
            repository.rename_workspace_group(workspace, group_id, name, &created),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        repository.remove_workspace_group_member("", GROUP_ID, "user_group", &created),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.deactivate_workspace_group("", GROUP_ID, 41, &created),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn inner_group_mutations_reject_invalid_fields_audits_and_timestamps() {
    let (_temporary, repository) = repository();
    let group = group();
    for invalid in [
        member("group_invalid", "workspace_fixture", "user_group", 41),
        member(GROUP_ID, "workspace_fixture", "user_group", u64::MAX),
    ] {
        assert_eq!(
            repository.add_workspace_group_member(
                &invalid,
                &event(
                    "audit_invalid_add",
                    "group.member_added",
                    &group,
                    invalid.added_at_ms,
                    Some("user_group"),
                ),
            ),
            Err(RepositoryError::InvalidInput)
        );
    }
    for (group_id, user_id) in [("group_invalid", "user_group"), (GROUP_ID, "")] {
        assert_eq!(
            repository.remove_workspace_group_member(
                "workspace_fixture",
                group_id,
                user_id,
                &event(
                    "audit_invalid_remove",
                    "group.member_removed",
                    &group,
                    41,
                    Some(user_id),
                ),
            ),
            Err(RepositoryError::InvalidInput)
        );
    }

    let mut renamed = event("audit_invalid_rename", "group.renamed", &group, 41, None);
    renamed.action = "wrong.action".to_owned();
    assert_eq!(
        repository.rename_workspace_group("workspace_fixture", GROUP_ID, "Approvers", &renamed),
        Err(RepositoryError::InvalidInput)
    );
    let mut deactivated = event(
        "audit_invalid_deactivate",
        "group.deactivated",
        &group,
        42,
        None,
    );
    deactivated.action = "wrong.action".to_owned();
    assert_eq!(
        repository.deactivate_workspace_group("workspace_fixture", GROUP_ID, 42, &deactivated),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.deactivate_workspace_group(
            "workspace_fixture",
            GROUP_ID,
            u64::MAX,
            &event(
                "audit_invalid_deactivate_time",
                "group.deactivated",
                &group,
                u64::MAX,
                None,
            ),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn private_group_mutation_boundary_revalidates_identifiers_and_names() {
    let (_temporary, repository) = repository();
    let group = group();
    let audit = event("audit_private_boundary", "group.renamed", &group, 41, None);
    let mut connection = repository.connection.lock().expect("connection");
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(
        super::super::workspace_group_mutations::rename(
            &transaction,
            "workspace_fixture",
            "group_invalid",
            "Approvers",
            &audit,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        super::super::workspace_group_mutations::rename(
            &transaction,
            "workspace_fixture",
            GROUP_ID,
            "x",
            &audit,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        super::super::workspace_group_mutations::deactivate(
            &transaction,
            "workspace_fixture",
            "group_invalid",
            41,
            &audit,
        ),
        Err(RepositoryError::InvalidInput)
    );
    drop(transaction);
    drop(connection);
}

fn member(
    group_id: &str,
    workspace_id: &str,
    user_id: &str,
    added_at_ms: u64,
) -> WorkspaceGroupMemberRecord {
    WorkspaceGroupMemberRecord {
        group_id: group_id.to_owned(),
        workspace_id: workspace_id.to_owned(),
        user_id: user_id.to_owned(),
        added_at_ms,
    }
}
