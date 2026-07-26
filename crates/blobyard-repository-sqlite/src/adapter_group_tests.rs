#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::group_repository as repository;
use blobyard_contract::{
    AuditValue, LifecycleRepository, LocalUserRepository, NewYardAccessGrant, RepositoryError,
    WebYardRepository, WorkspaceGroupMemberRecord, WorkspaceGroupRecord, WorkspaceGroupRepository,
    WorkspaceGroupStatus, YardAccessPrincipalKind,
};

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

#[test]
fn group_lifecycle_is_atomic_cursor_paginated_and_exactly_audited() {
    let (_temporary, repository) = repository();
    let first = group(1, 40);
    let second = group(2, 40);
    create(&repository, &first, 40);
    create(&repository, &second, 40);
    let page = repository
        .list_workspace_groups("workspace_fixture", None, 1)
        .expect("first page");
    assert_eq!(page.items, vec![second]);
    let next = repository
        .list_workspace_groups("workspace_fixture", page.next_cursor.as_ref(), 1)
        .expect("second page");
    assert_eq!(next.items, vec![first.clone()]);
    assert!(next.next_cursor.is_none());

    let renamed = repository
        .rename_workspace_group(
            "workspace_fixture",
            &first.id,
            "Renamed group",
            &blobyard_testkit::group_event(
                "audit_group_rename",
                "group.renamed",
                &first,
                41,
                [("to", AuditValue::String("Renamed group".to_owned()))],
            ),
        )
        .expect("rename");
    assert_eq!(renamed.name, "Renamed group");
    let audit = repository
        .list_audit("workspace_fixture", None, 20)
        .expect("audit");
    let rename = audit
        .items
        .iter()
        .find(|event| event.id == "audit_group_rename")
        .expect("rename audit");
    assert!(
        rename
            .metadata
            .contains(&("from".to_owned(), AuditValue::String(first.name)))
    );
}

#[test]
fn membership_changes_update_counts_and_user_deactivation_removes_memberships() {
    let (_temporary, repository) = repository();
    let group = group(3, 40);
    create(&repository, &group, 40);
    let member = WorkspaceGroupMemberRecord {
        group_id: group.id.clone(),
        workspace_id: group.workspace_id.clone(),
        user_id: "user_group".to_owned(),
        added_at_ms: 41,
    };
    repository
        .add_workspace_group_member(
            &member,
            &blobyard_testkit::group_event(
                "audit_member_add",
                "group.member_added",
                &group,
                41,
                [("userId", AuditValue::String(member.user_id.clone()))],
            ),
        )
        .expect("member");
    assert_eq!(
        repository
            .list_workspace_groups("workspace_fixture", None, 50)
            .expect("groups")
            .items[0]
            .member_count,
        1
    );
    let user = repository
        .list_local_users("workspace_fixture")
        .expect("users")
        .into_iter()
        .find(|listing| listing.user.id == "user_group")
        .expect("user")
        .user;
    repository
        .deactivate_local_user(
            &user.id,
            42,
            &blobyard_testkit::local_user_event(
                "audit_group_user_deactivate",
                &user,
                "user.deactivated",
                42,
            ),
        )
        .expect("deactivate");
    assert!(
        repository
            .list_workspace_group_members("workspace_fixture", &group.id, None, 50)
            .expect("members")
            .items
            .is_empty()
    );
}

#[test]
fn deactivation_revokes_group_grants_once_and_denies_repeated_mutation() {
    let (_temporary, repository) = repository();
    let group = group(4, 40);
    create(&repository, &group, 40);
    let yard = seed_yard(&repository);
    let grant = NewYardAccessGrant {
        id: "grant_group_fixture".to_owned(),
        yard_id: yard.id.clone(),
        environment_id: None,
        principal_kind: YardAccessPrincipalKind::Group,
        principal_id: group.id.clone(),
        app_roles: vec!["viewer".to_owned()],
        created_at_ms: 42,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms: None,
    };
    repository
        .insert_yard_access_grant(
            &grant,
            &blobyard_testkit::granted_event(&yard.id, &grant, 42),
        )
        .expect("group grant");
    let deactivated = blobyard_testkit::group_event(
        "audit_group_deactivate",
        "group.deactivated",
        &group,
        43,
        [],
    );
    repository
        .deactivate_workspace_group("workspace_fixture", &group.id, 43, &deactivated)
        .expect("deactivate group");
    assert!(
        repository
            .list_yard_access_grants(&yard.id, 43)
            .expect("grants")
            .is_empty()
    );
    assert_eq!(
        repository.deactivate_workspace_group("workspace_fixture", &group.id, 44, &deactivated),
        Err(RepositoryError::Conflict)
    );
    let audit = repository
        .list_audit("workspace_fixture", None, 20)
        .expect("audit");
    let event = audit
        .items
        .iter()
        .find(|event| event.id == deactivated.id)
        .expect("deactivation audit");
    assert!(
        event
            .metadata
            .contains(&("revokedGrantCount".to_owned(), AuditValue::Number(1)))
    );
}

fn create(repository: &super::SqliteRepository, group: &WorkspaceGroupRecord, at: u64) {
    repository
        .create_workspace_group(
            group,
            &blobyard_testkit::group_event(
                &format!("audit_group_{}", group.id),
                "group.created",
                group,
                at,
                [("name", AuditValue::String(group.name.clone()))],
            ),
        )
        .expect("group");
}

fn seed_yard(repository: &super::SqliteRepository) -> blobyard_contract::NewWebYard {
    let yard = blobyard_contract::NewWebYard {
        id: "yard_group_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: blobyard_core::Slug::new("group-yard").expect("slug"),
        host_label: "group-yard-123456789-fixture".to_owned(),
        created_at_ms: 41,
    };
    let deploy = blobyard_contract::NewYardDeploy {
        id: "deploy_group_fixture".to_owned(),
        yard_id: yard.id.clone(),
        workspace_id: yard.workspace_id.clone(),
        project_id: yard.project_id.clone(),
        client_deploy_id: "clientdeploy00000041".to_owned(),
        manifest_root: format!(".blobyard-yard/{}/clientdeploy00000041/", yard.id),
        deployment_host_label: "group-yard-0123456789-fixture".to_owned(),
        spa: true,
        clean_urls: true,
        created_at_ms: 41,
    };
    repository
        .start_yard_deploy(
            &yard,
            &deploy,
            &blobyard_testkit::yard_event("yard.created", "web_yard", "yardId", &yard.id, 41),
        )
        .expect("yard");
    yard
}
