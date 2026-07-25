#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::empty_repository;
use blobyard_contract::{
    AuditValue, NewYardAccessGrant, RepositoryError, WebYardRepository, WorkspaceGroupMemberRecord,
    WorkspaceGroupRecord, WorkspaceGroupRepository, WorkspaceGroupStatus, YardAccessPrincipalKind,
};

fn group(number: u64) -> WorkspaceGroupRecord {
    WorkspaceGroupRecord {
        id: format!("group_{number:032x}"),
        workspace_id: "workspace_fixture".to_owned(),
        name: format!("Group {number}"),
        status: WorkspaceGroupStatus::Active,
        member_count: 0,
        created_at_ms: number + 100,
        deactivated_at_ms: None,
    }
}

fn repository() -> (tempfile::TempDir, super::SqliteRepository) {
    let (temporary, repository) = empty_repository();
    blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
    let connection = repository.test_connection().expect("connection");
    connection
        .execute(
            "INSERT INTO local_users VALUES ('user_limit', 'workspace_fixture', 'Limit User', NULL, 'active', 1, NULL)",
            [],
        )
        .expect("limit user");
    drop(connection);
    (temporary, repository)
}

fn create_event(group: &WorkspaceGroupRecord) -> blobyard_contract::NewAuditEvent {
    blobyard_testkit::group_event(
        &format!("audit_create_{}", group.id),
        "group.created",
        group,
        group.created_at_ms,
        [("name", AuditValue::String(group.name.clone()))],
    )
}

fn member_event(
    group: &WorkspaceGroupRecord,
    user_id: &str,
    at_ms: u64,
) -> blobyard_contract::NewAuditEvent {
    blobyard_testkit::group_event(
        &format!("audit_member_{}_{}", group.id, user_id),
        "group.member_added",
        group,
        at_ms,
        [("userId", AuditValue::String(user_id.to_owned()))],
    )
}

#[test]
fn workspace_group_limit_counts_only_active_groups() {
    let (_temporary, repository) = repository();
    let connection = repository.test_connection().expect("connection");
    connection
        .execute_batch(
            "WITH RECURSIVE numbers(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 500
             )
             INSERT INTO workspace_groups
               (id, workspace_id, name, status, member_count, created_at_ms, deactivated_at_ms)
             SELECT printf('group_%032x', value), 'workspace_fixture',
                    printf('Group %d', value), 'active', 0, value + 100, NULL
             FROM numbers;",
        )
        .expect("five hundred groups");
    drop(connection);
    let overflow = group(501);
    assert_eq!(
        repository.create_workspace_group(&overflow, &create_event(&overflow)),
        Err(RepositoryError::Conflict)
    );
    let connection = repository.test_connection().expect("connection");
    connection
        .execute(
            "UPDATE workspace_groups SET status = 'deactivated', deactivated_at_ms = 600 WHERE id = 'group_00000000000000000000000000000001'",
            [],
        )
        .expect("deactivate one");
    drop(connection);
    repository
        .create_workspace_group(&overflow, &create_event(&overflow))
        .expect("replacement active group");
}

#[test]
fn member_limits_reject_the_five_hundred_first_member_and_one_hundred_first_group() {
    let (_temporary, repository) = repository();
    let full_group = group(600);
    let user_full_group = group(701);
    let connection = repository.test_connection().expect("connection");
    connection
        .execute_batch(
            "INSERT INTO workspace_groups VALUES (
               'group_00000000000000000000000000000258', 'workspace_fixture',
               'Full group', 'active', 500, 700, NULL
             );
             WITH RECURSIVE numbers(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 100
             )
             INSERT INTO workspace_groups
               (id, workspace_id, name, status, member_count, created_at_ms, deactivated_at_ms)
             SELECT printf('group_%032x', value + 800), 'workspace_fixture',
                    printf('Membership %d', value), 'active', 1, value + 800, NULL
             FROM numbers;
             INSERT INTO workspace_groups VALUES (
               'group_000000000000000000000000000002bd', 'workspace_fixture',
               'User overflow', 'active', 0, 901, NULL
             );
             WITH RECURSIVE numbers(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 100
             )
             INSERT INTO workspace_group_members
               (group_id, workspace_id, user_id, added_at_ms)
             SELECT printf('group_%032x', value + 800), 'workspace_fixture',
                    'user_limit', value + 800
             FROM numbers;",
        )
        .expect("membership limits");
    drop(connection);
    let full_member = member(&full_group, "user_limit", 701);
    assert_eq!(
        repository.add_workspace_group_member(
            &full_member,
            &member_event(&full_group, "user_limit", 701)
        ),
        Err(RepositoryError::Conflict)
    );
    let overflow_member = member(&user_full_group, "user_limit", 902);
    assert_eq!(
        repository.add_workspace_group_member(
            &overflow_member,
            &member_event(&user_full_group, "user_limit", 902)
        ),
        Err(RepositoryError::Conflict)
    );
}

#[test]
fn membership_and_grants_reject_cross_workspace_principals() {
    let (_temporary, repository) = repository();
    let target = group(1_000);
    repository
        .create_workspace_group(&target, &create_event(&target))
        .expect("target group");
    let connection = repository.test_connection().expect("connection");
    connection
        .execute_batch(
            "INSERT INTO workspaces VALUES ('other_workspace', 'Other', 'other');
             INSERT INTO local_users VALUES (
               'other_user', 'other_workspace', 'Other User', NULL, 'active', 1, NULL
             );",
        )
        .expect("other workspace");
    drop(connection);
    let cross_member = member(&target, "other_user", 1_002);
    assert_eq!(
        repository
            .add_workspace_group_member(&cross_member, &member_event(&target, "other_user", 1_002)),
        Err(RepositoryError::NotFound)
    );
    let yard = seed_yard(&repository);
    let other_group = WorkspaceGroupRecord {
        id: "group_000000000000000000000000000003e9".to_owned(),
        workspace_id: "other_workspace".to_owned(),
        name: "Other group".to_owned(),
        status: WorkspaceGroupStatus::Active,
        member_count: 0,
        created_at_ms: 1_003,
        deactivated_at_ms: None,
    };
    repository
        .create_workspace_group(&other_group, &create_event(&other_group))
        .expect("other group");
    let grant = grant(&yard.id, &other_group.id, 1_004);
    assert_eq!(
        repository.insert_yard_access_grant(
            &grant,
            &blobyard_testkit::granted_event(&yard.id, &grant, 1_004)
        ),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn expired_active_group_grants_still_consume_the_five_hundred_grant_limit() {
    let (_temporary, repository) = repository();
    let target = group(1_100);
    repository
        .create_workspace_group(&target, &create_event(&target))
        .expect("target group");
    let yard = seed_yard(&repository);
    let connection = repository.test_connection().expect("connection");
    connection
        .execute_batch(&format!(
            "WITH RECURSIVE numbers(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 500
             )
             INSERT INTO yard_access_grants
               (id, yard_id, environment_id, principal_kind, principal_id, app_roles,
                status, created_at_ms, created_by_principal, expires_at_ms, revoked_at_ms)
             SELECT printf('grant_group_limit_%03d', value), '{}', NULL, 'group', '{}',
                    '[\"viewer\"]', 'active', 1, 'fixture', 1, NULL
             FROM numbers;",
            yard.id, target.id
        ))
        .expect("five hundred active grants");
    drop(connection);
    let overflow = grant(&yard.id, &target.id, 1_102);
    assert_eq!(
        repository.insert_yard_access_grant(
            &overflow,
            &blobyard_testkit::granted_event(&yard.id, &overflow, 1_102)
        ),
        Err(RepositoryError::Conflict)
    );
    let connection = repository.test_connection().expect("connection");
    connection
        .execute(
            "INSERT INTO yard_access_grants
               (id, yard_id, environment_id, principal_kind, principal_id, app_roles,
                status, created_at_ms, created_by_principal, expires_at_ms, revoked_at_ms)
             VALUES ('grant_group_limit_501', ?1, NULL, 'group', ?2, '[\"viewer\"]',
                     'active', 1, 'fixture', 1, NULL)",
            rusqlite::params![yard.id, target.id],
        )
        .expect("grant above deactivation bound");
    drop(connection);
    assert_eq!(
        repository.deactivate_workspace_group(
            "workspace_fixture",
            &target.id,
            1_103,
            &blobyard_testkit::group_event(
                "audit_deactivate_above_grant_limit",
                "group.deactivated",
                &target,
                1_103,
                [],
            ),
        ),
        Err(RepositoryError::Conflict)
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

fn grant(yard_id: &str, group_id: &str, at_ms: u64) -> NewYardAccessGrant {
    NewYardAccessGrant {
        id: format!("grant_group_{at_ms}"),
        yard_id: yard_id.to_owned(),
        environment_id: None,
        principal_kind: YardAccessPrincipalKind::Group,
        principal_id: group_id.to_owned(),
        app_roles: vec!["viewer".to_owned()],
        created_at_ms: at_ms,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms: None,
    }
}

fn seed_yard(repository: &super::SqliteRepository) -> blobyard_contract::NewWebYard {
    let yard = blobyard_contract::NewWebYard {
        id: "yard_group_limit_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: blobyard_core::Slug::new("group-limit").expect("slug"),
        host_label: "group-limit-123456789-fixture".to_owned(),
        created_at_ms: 1_001,
    };
    let deploy = blobyard_contract::NewYardDeploy {
        id: format!("deploy_{}", yard.id),
        yard_id: yard.id.clone(),
        workspace_id: yard.workspace_id.clone(),
        project_id: yard.project_id.clone(),
        client_deploy_id: "clientdeploy00001001".to_owned(),
        manifest_root: format!(".blobyard-yard/{}/clientdeploy00001001/", yard.id),
        deployment_host_label: "group-limit-0123456789-fixture".to_owned(),
        spa: true,
        clean_urls: true,
        created_at_ms: 1_001,
    };
    repository
        .start_yard_deploy(
            &yard,
            &deploy,
            &blobyard_testkit::yard_event("yard.created", "web_yard", "yardId", &yard.id, 1_001),
        )
        .expect("yard");
    yard
}
