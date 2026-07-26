use super::{
    audit_count, create_event, group, member, member_event, repository, require_audit_unchanged,
    seed_yard,
};
use blobyard_contract::{LifecycleRepository, RepositoryError, WorkspaceGroupRepository};

pub(super) fn workspace_group_limit_counts_only_active_groups(
    tracker: &mut blobyard_testkit::FixtureExecutionTracker,
) {
    let (_temporary, repository) = repository();
    repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "WITH RECURSIVE numbers(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 499
             )
             INSERT INTO workspace_groups
               (id, workspace_id, name, status, member_count, created_at_ms, deactivated_at_ms)
             SELECT printf('group_%032x', value), 'workspace_fixture',
                    printf('Group %d', value), 'active', 0, value + 100, NULL
             FROM numbers;",
        )
        .expect("four hundred ninety-nine groups");
    let last = group(500);
    repository
        .create_workspace_group(&last, &create_event(&last))
        .expect("five hundredth active group");
    let overflow = group(501);
    let audit_before = audit_count(&repository);
    assert_eq!(
        repository.create_workspace_group(&overflow, &create_event(&overflow)),
        Err(RepositoryError::Conflict)
    );
    assert_audit_unchanged(&repository, audit_before, &create_event(&overflow).id);
    assert_eq!(
        scalar_count(
            &repository,
            "SELECT COUNT(*) FROM workspace_groups WHERE workspace_id = 'workspace_fixture' AND status = 'active'",
        ),
        500
    );
    fixture(
        tracker,
        "active-group-limit-accepts-last-and-rejects-next",
        "active-groups-per-workspace",
        500,
    );
}

pub(super) fn member_limit_accepts_five_hundredth_and_rejects_next(
    tracker: &mut blobyard_testkit::FixtureExecutionTracker,
) {
    let (_temporary, repository) = repository();
    let full_group = group(600);
    repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "INSERT INTO workspace_groups VALUES (
               'group_00000000000000000000000000000258', 'workspace_fixture',
               'Full group', 'active', 499, 700, NULL
             );
             WITH RECURSIVE numbers(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 499
             )
             INSERT INTO local_users
               (id, workspace_id, display_name, email, status, created_at_ms, deactivated_at_ms)
             SELECT printf('user_member_%03d', value), 'workspace_fixture',
                    printf('Member %d', value), NULL, 'active', value, NULL
             FROM numbers;
             WITH RECURSIVE numbers(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 499
             )
             INSERT INTO workspace_group_members
               (group_id, workspace_id, user_id, added_at_ms)
             SELECT 'group_00000000000000000000000000000258', 'workspace_fixture',
                    printf('user_member_%03d', value), value
             FROM numbers;
             INSERT INTO local_users VALUES
               ('user_limit_overflow', 'workspace_fixture', 'Overflow User', NULL, 'active', 1, NULL);",
        )
        .expect("member limit");
    repository
        .add_workspace_group_member(
            &member(&full_group, "user_limit", 701),
            &member_event(&full_group, "user_limit", 701),
        )
        .expect("five hundredth member");
    let overflow = member(&full_group, "user_limit_overflow", 702);
    let overflow_event = member_event(&full_group, "user_limit_overflow", 702);
    let audit_before = audit_count(&repository);
    assert_eq!(
        repository.add_workspace_group_member(&overflow, &overflow_event),
        Err(RepositoryError::Conflict)
    );
    assert_audit_unchanged(&repository, audit_before, &overflow_event.id);
    assert_count(
        &repository,
        "SELECT COUNT(*) FROM workspace_group_members WHERE group_id = 'group_00000000000000000000000000000258'",
        500,
    );
    fixture(
        tracker,
        "active-member-limit-accepts-last-and-rejects-next",
        "active-members-per-group",
        500,
    );
}

pub(super) fn membership_limit_accepts_one_hundredth_and_rejects_next(
    tracker: &mut blobyard_testkit::FixtureExecutionTracker,
) {
    let (_temporary, repository) = repository();
    let last_group = group(701);
    let overflow_group = group(702);
    repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "WITH RECURSIVE numbers(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 99
             )
             INSERT INTO workspace_groups
               (id, workspace_id, name, status, member_count, created_at_ms, deactivated_at_ms)
             SELECT printf('group_%032x', value + 800), 'workspace_fixture',
                    printf('Membership %d', value), 'active', 1, value + 800, NULL
             FROM numbers;
             WITH RECURSIVE numbers(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 99
             )
             INSERT INTO workspace_group_members
               (group_id, workspace_id, user_id, added_at_ms)
             SELECT printf('group_%032x', value + 800), 'workspace_fixture',
                    'user_limit', value + 800
             FROM numbers;",
        )
        .expect("ninety-nine memberships");
    create_group(&repository, &last_group, "last group");
    create_group(&repository, &overflow_group, "overflow group");
    repository
        .add_workspace_group_member(
            &member(&last_group, "user_limit", 902),
            &member_event(&last_group, "user_limit", 902),
        )
        .expect("one hundredth membership");
    let overflow = member(&overflow_group, "user_limit", 903);
    let overflow_event = member_event(&overflow_group, "user_limit", 903);
    let audit_before = audit_count(&repository);
    assert_eq!(
        repository.add_workspace_group_member(&overflow, &overflow_event),
        Err(RepositoryError::Conflict)
    );
    assert_audit_unchanged(&repository, audit_before, &overflow_event.id);
    assert_eq!(
        scalar_count(
            &repository,
            "SELECT COUNT(*) FROM active_workspace_group_members WHERE user_id = 'user_limit'",
        ),
        100
    );
    fixture(
        tracker,
        "active-membership-limit-accepts-last-and-rejects-next",
        "active-memberships-per-user",
        100,
    );
}

#[test]
fn group_deactivation_fails_closed_when_store_exceeds_grant_limit() {
    let (_temporary, repository) = repository();
    let target = group(1_200);
    create_group(&repository, &target, "target group");
    let yard = seed_yard(&repository);
    repository
        .test_connection()
        .expect("connection")
        .execute_batch(&format!(
            "WITH RECURSIVE numbers(value) AS (
               SELECT 1 UNION ALL SELECT value + 1 FROM numbers WHERE value < 501
             )
             INSERT INTO yard_access_grants
               (id, yard_id, environment_id, principal_kind, principal_id, app_roles,
                status, created_at_ms, created_by_principal, expires_at_ms, revoked_at_ms)
             SELECT printf('grant_group_corrupt_%03d', value), '{}', NULL, 'group', '{}',
                    '[\"viewer\"]', 'active', value, 'fixture', NULL, NULL
             FROM numbers;",
            yard.id, target.id
        ))
        .expect("corrupt active grant inventory");
    let event = blobyard_testkit::group_event(
        "audit_group_corrupt_deactivate",
        "group.deactivated",
        &target,
        1_202,
        [],
    );

    let audit_before = audit_count(&repository);
    assert_eq!(
        repository.deactivate_workspace_group("workspace_fixture", &target.id, 1_202, &event),
        Err(RepositoryError::Conflict)
    );
    assert_audit_unchanged(&repository, audit_before, &event.id);
    assert_eq!(
        scalar_count(
            &repository,
            "SELECT COUNT(*) FROM yard_access_grants WHERE status = 'active' AND principal_kind = 'group'",
        ),
        501
    );
}

#[test]
fn total_audit_guard_rejects_a_differently_identified_event() {
    let (_temporary, repository) = repository();
    let before = audit_count(&repository);
    repository
        .record_audit(&blobyard_testkit::group_event(
            "audit_different_id",
            "group.created",
            &group(990),
            990,
            [],
        ))
        .expect("unexpected audit");
    assert_eq!(
        require_audit_unchanged(&repository, before, "audit_supplied_overflow"),
        Err(RepositoryError::Unavailable)
    );
}

fn fixture(
    tracker: &mut blobyard_testkit::FixtureExecutionTracker,
    id: &str,
    boundary: &str,
    limit: u64,
) {
    tracker.record_case(
        id,
        &serde_json::json!({"boundary": boundary, "limit": limit}),
        &serde_json::json!({
            "lastWrite": "accepted",
            "overflowCode": "CONFLICT",
            "auditEventsForOverflow": 0
        }),
    );
}

fn assert_audit_unchanged(repository: &crate::SqliteRepository, before: usize, event_id: &str) {
    require_audit_unchanged(repository, before, event_id).expect("rejected audit unchanged");
}

fn create_group(
    repository: &crate::SqliteRepository,
    value: &blobyard_contract::WorkspaceGroupRecord,
    message: &str,
) {
    repository
        .create_workspace_group(value, &create_event(value))
        .expect(message);
}

fn scalar_count(repository: &crate::SqliteRepository, sql: &str) -> i64 {
    repository
        .test_connection()
        .expect("connection")
        .query_row(sql, [], |row| row.get(0))
        .expect("count")
}

fn assert_count(repository: &crate::SqliteRepository, sql: &str, expected: i64) {
    assert_eq!(scalar_count(repository, sql), expected);
}
