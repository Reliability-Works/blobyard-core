#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
#![allow(
    clippy::too_many_lines,
    reason = "group rollback tests keep each atomic mutation sequence together"
)]

use super::group_repository as repository;
use blobyard_contract::{
    AuditValue, LifecycleRepository, RepositoryError, WorkspaceGroupCursor,
    WorkspaceGroupMemberCursor, WorkspaceGroupMemberRecord, WorkspaceGroupRecord,
    WorkspaceGroupRepository, WorkspaceGroupStatus,
};
use std::sync::atomic::Ordering;

fn group(number: u64, created_at_ms: u64) -> WorkspaceGroupRecord {
    WorkspaceGroupRecord {
        id: format!("group_{number:032x}"),
        workspace_id: "workspace_fixture".to_owned(),
        name: format!("Group {number}"),
        status: WorkspaceGroupStatus::Active,
        member_count: 0,
        created_at_ms,
        deactivated_at_ms: None,
    }
}

fn create(repository: &super::SqliteRepository, group: &WorkspaceGroupRecord) {
    repository
        .create_workspace_group(
            group,
            &blobyard_testkit::group_event(
                &format!("audit_group_{}", group.id),
                "group.created",
                group,
                group.created_at_ms,
                [("name", AuditValue::String(group.name.clone()))],
            ),
        )
        .expect("group");
}

#[test]
fn invalid_group_audits_roll_back_create_and_membership_mutations() {
    let (_temporary, repository) = repository();
    let group = group(5, 40);
    let mut invalid_create = blobyard_testkit::group_event(
        "audit_invalid_create",
        "group.created",
        &group,
        40,
        [("name", AuditValue::String(group.name.clone()))],
    );
    invalid_create.action = "wrong.action".to_owned();
    assert_eq!(
        repository.create_workspace_group(&group, &invalid_create),
        Err(RepositoryError::InvalidInput)
    );
    assert!(
        repository
            .list_workspace_groups("workspace_fixture", None, 50)
            .expect("groups")
            .items
            .is_empty()
    );
    create(&repository, &group);

    let member = member(&group, "user_group", 41);
    let mut invalid_add = member_event(&group, &member, "group.member_added");
    invalid_add.action = "wrong.action".to_owned();
    assert_eq!(
        repository.add_workspace_group_member(&member, &invalid_add),
        Err(RepositoryError::InvalidInput)
    );
    assert!(
        repository
            .list_workspace_group_members("workspace_fixture", &group.id, None, 50)
            .expect("members")
            .items
            .is_empty()
    );
    assert_eq!(
        repository.remove_workspace_group_member(
            "workspace_fixture",
            &group.id,
            "user_missing",
            &blobyard_testkit::group_event(
                "audit_missing_remove",
                "group.member_removed",
                &group,
                42,
                [("userId", AuditValue::String("user_missing".to_owned()))],
            ),
        ),
        Err(RepositoryError::NotFound)
    );

    repository
        .add_workspace_group_member(
            &member,
            &member_event(&group, &member, "group.member_added"),
        )
        .expect("member");
    let mut invalid_remove = member_event(&group, &member, "group.member_removed");
    invalid_remove.action = "wrong.action".to_owned();
    assert_eq!(
        repository.remove_workspace_group_member(
            "workspace_fixture",
            &group.id,
            &member.user_id,
            &invalid_remove,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository
            .list_workspace_group_members("workspace_fixture", &group.id, None, 50)
            .expect("members")
            .items,
        vec![member]
    );
}

#[test]
fn injected_mid_transaction_faults_roll_back_group_and_audit_together() {
    let first_success = (0..16).find(|denied_index| {
        let (_temporary, repository) = repository();
        let group = group(50, 40);
        let event = blobyard_testkit::group_event(
            "audit_group_injected_rollback",
            "group.created",
            &group,
            40,
            [("name", AuditValue::String(group.name.clone()))],
        );
        let observed = {
            let connection = repository.test_connection().expect("connection");
            super::install_denial(&connection, *denied_index)
        };
        let result = repository.create_workspace_group(&group, &event);
        if observed.load(Ordering::Relaxed) <= *denied_index {
            assert_eq!(result, Ok(()));
            true
        } else {
            assert_eq!(result, Err(RepositoryError::Unavailable));
            let groups = repository
                .list_workspace_groups("workspace_fixture", None, 50)
                .expect("groups");
            let audits = repository
                .list_audit("workspace_fixture", None, 50)
                .expect("audits");
            assert!(groups.items.iter().all(|item| item.id != group.id));
            assert!(audits.items.iter().all(|item| item.id != event.id));
            false
        }
    });
    assert!(first_success.is_some(), "fault sweep must terminate");
}

#[test]
fn group_validation_cursors_limits_and_member_pagination_fail_closed() {
    let (_temporary, repository) = repository();
    let target_group = group(6, 40);
    create(&repository, &target_group);
    let invalid_group_cursor = WorkspaceGroupCursor {
        created_at_ms: u64::MAX,
        id: target_group.id.clone(),
    };
    assert_eq!(
        repository.list_workspace_groups("workspace_fixture", Some(&invalid_group_cursor), 50),
        Err(RepositoryError::InvalidInput)
    );
    for limit in [0, 51] {
        assert_eq!(
            repository.list_workspace_groups("workspace_fixture", None, limit),
            Err(RepositoryError::InvalidInput)
        );
        assert_eq!(
            repository.list_workspace_group_members(
                "workspace_fixture",
                &target_group.id,
                None,
                limit,
            ),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_invalid_group_mutations(&repository, &target_group);
    assert_member_pagination(&repository, &target_group);
}

fn assert_invalid_group_mutations(
    repository: &super::SqliteRepository,
    target_group: &WorkspaceGroupRecord,
) {
    assert_eq!(
        repository.rename_workspace_group(
            "workspace_fixture",
            &target_group.id,
            "e\u{301}quipe",
            &blobyard_testkit::group_event(
                "audit_unnormalized_rename",
                "group.renamed",
                target_group,
                41,
                [("to", AuditValue::String("équipe".to_owned()))],
            ),
        ),
        Err(RepositoryError::InvalidInput)
    );
    let mut invalid_group = group(7, 42);
    invalid_group.status = WorkspaceGroupStatus::Deactivated;
    assert_eq!(
        repository.create_workspace_group(
            &invalid_group,
            &blobyard_testkit::group_event(
                "audit_invalid_group",
                "group.created",
                &invalid_group,
                42,
                [("name", AuditValue::String(invalid_group.name.clone()))],
            ),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_workspace_group_members("workspace_fixture", "group_invalid", None, 50),
        Err(RepositoryError::InvalidInput)
    );
}

fn assert_member_pagination(
    repository: &super::SqliteRepository,
    target_group: &WorkspaceGroupRecord,
) {
    seed_additional_group_users(repository);
    let first = member(target_group, "user_group_2", 42);
    let second = member(target_group, "user_group_3", 43);
    for value in [&first, &second] {
        repository
            .add_workspace_group_member(
                value,
                &member_event(target_group, value, "group.member_added"),
            )
            .expect("member");
    }
    let page = repository
        .list_workspace_group_members("workspace_fixture", &target_group.id, None, 1)
        .expect("first member page");
    assert_eq!(page.items, vec![second]);
    let next = repository
        .list_workspace_group_members(
            "workspace_fixture",
            &target_group.id,
            page.next_cursor.as_ref(),
            1,
        )
        .expect("second member page");
    assert_eq!(next.items, vec![first]);
    let invalid_cursor = WorkspaceGroupMemberCursor {
        added_at_ms: u64::MAX,
        user_id: "user_group_3".to_owned(),
    };
    assert_eq!(
        repository.list_workspace_group_members(
            "workspace_fixture",
            &target_group.id,
            Some(&invalid_cursor),
            50,
        ),
        Err(RepositoryError::InvalidInput)
    );
}

fn member(
    group: &WorkspaceGroupRecord,
    user_id: &str,
    added_at_ms: u64,
) -> WorkspaceGroupMemberRecord {
    WorkspaceGroupMemberRecord {
        group_id: group.id.clone(),
        workspace_id: group.workspace_id.clone(),
        user_id: user_id.to_owned(),
        added_at_ms,
    }
}

fn member_event(
    group: &WorkspaceGroupRecord,
    member: &WorkspaceGroupMemberRecord,
    action: &str,
) -> blobyard_contract::NewAuditEvent {
    blobyard_testkit::group_event(
        &format!("audit_{}_{}", action.replace('.', "_"), member.user_id),
        action,
        group,
        member.added_at_ms,
        [("userId", AuditValue::String(member.user_id.clone()))],
    )
}

fn seed_additional_group_users(repository: &super::SqliteRepository) {
    let connection = repository.test_connection().expect("connection");
    connection
        .execute_batch(
            "INSERT INTO local_users VALUES
               ('user_group_2', 'workspace_fixture', 'Second User', NULL, 'active', 31, NULL),
               ('user_group_3', 'workspace_fixture', 'Third User', NULL, 'active', 32, NULL);",
        )
        .expect("additional group users");
}
