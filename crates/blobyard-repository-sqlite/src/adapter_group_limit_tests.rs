#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::empty_repository;
use blobyard_contract::{
    AuditValue, LifecycleRepository, NewYardAccessGrant, RepositoryError, WebYardRepository,
    WorkspaceGroupMemberRecord, WorkspaceGroupRecord, WorkspaceGroupRepository,
    WorkspaceGroupStatus, YardAccessPrincipalKind,
};

#[path = "adapter_group_limit_boundary_tests.rs"]
mod boundary_tests;

pub(super) fn group(number: u64) -> WorkspaceGroupRecord {
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

pub(super) fn repository() -> (tempfile::TempDir, super::SqliteRepository) {
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

pub(super) fn create_event(group: &WorkspaceGroupRecord) -> blobyard_contract::NewAuditEvent {
    blobyard_testkit::group_event(
        &format!("audit_create_{}", group.id),
        "group.created",
        group,
        group.created_at_ms,
        [("name", AuditValue::String(group.name.clone()))],
    )
}

pub(super) fn member_event(
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
fn group_limit_suite_executes_every_generated_case() {
    let mut tracker = blobyard_testkit::FixtureExecutionTracker::new("sqlite", "group-limits");
    group_grant_limit_accepts_five_hundredth_and_rejects_next(&mut tracker);
    boundary_tests::workspace_group_limit_counts_only_active_groups(&mut tracker);
    boundary_tests::member_limit_accepts_five_hundredth_and_rejects_next(&mut tracker);
    boundary_tests::membership_limit_accepts_one_hundredth_and_rejects_next(&mut tracker);
    tracker.finish().expect("complete group limit fixtures");
}

fn group_grant_limit_accepts_five_hundredth_and_rejects_next(
    tracker: &mut blobyard_testkit::FixtureExecutionTracker,
) {
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
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 499
             )
             INSERT INTO yard_access_grants
               (id, yard_id, environment_id, principal_kind, principal_id, app_roles,
                status, created_at_ms, created_by_principal, expires_at_ms, revoked_at_ms)
             SELECT printf('grant_group_limit_%03d', value), '{}', NULL, 'group', '{}',
                    '[\"viewer\"]', 'active', 1, 'fixture', 1, NULL
             FROM numbers;",
            yard.id, target.id
        ))
        .expect("four hundred ninety-nine active grants");
    drop(connection);
    let last = grant(&yard.id, &target.id, 1_102);
    repository
        .insert_yard_access_grant(
            &last,
            &blobyard_testkit::granted_event(&yard.id, &last, 1_102),
        )
        .expect("five hundredth active grant");
    let overflow = grant(&yard.id, &target.id, 1_103);
    let audit_before = audit_count(&repository);
    assert_eq!(
        repository.insert_yard_access_grant(
            &overflow,
            &blobyard_testkit::granted_event(&yard.id, &overflow, 1_103)
        ),
        Err(RepositoryError::Conflict)
    );
    require_audit_unchanged(
        &repository,
        audit_before,
        &blobyard_testkit::granted_event(&yard.id, &overflow, 1_103).id,
    )
    .expect("overflow audit unchanged");
    assert_eq!(active_grant_count(&repository, &target.id), 500);
    tracker.record_case(
        "active-group-grant-limit-accepts-last-and-rejects-next",
        &serde_json::json!({
            "boundary": "active-grants-per-group-principal",
            "limit": 500
        }),
        &serde_json::json!({
            "lastWrite": "accepted",
            "overflowCode": "CONFLICT",
            "auditEventsForOverflow": 0
        }),
    );
}

pub(super) fn audit_count(repository: &super::SqliteRepository) -> usize {
    repository
        .list_audit("workspace_fixture", None, 100)
        .expect("audits")
        .items
        .len()
}

pub(super) fn require_audit_unchanged(
    repository: &super::SqliteRepository,
    before: usize,
    event_id: &str,
) -> Result<(), RepositoryError> {
    let events = repository
        .list_audit("workspace_fixture", None, 100)
        .expect("audits")
        .items;
    if events.len() == before && events.iter().all(|event| event.id != event_id) {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

fn active_grant_count(repository: &super::SqliteRepository, group_id: &str) -> i64 {
    repository
        .test_connection()
        .expect("connection")
        .query_row(
            "SELECT COUNT(*) FROM yard_access_grants WHERE principal_kind = 'group' AND principal_id = ?1 AND status = 'active'",
            [group_id],
            |row| row.get(0),
        )
        .expect("grant count")
}

pub(super) fn member(
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
    super::approve_access_policy(repository, &yard.id, "user_limit", 1_002);
    yard
}
