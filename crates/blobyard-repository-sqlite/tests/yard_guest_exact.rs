#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Exact guest pagination and corruption fixture execution.

include!("yard_guest_exact/support.rs");

use blobyard_contract::{RepositoryError, YardGuestInviteCursor, YardSessionRepository};
use std::collections::BTreeSet;

#[test]
fn exact_guest_pagination_suite_executes_generated_case() {
    let fixture = Fixture::new();
    for index in 1..=51 {
        fixture.create(index);
    }
    let first = fixture
        .repository
        .list_yard_guest_invites("yard_guest", None, 50)
        .expect("first page");
    assert_eq!(first.items.len(), 50);
    assert_eq!(first.items[0].id, invitation_id(51));
    fixture.create(52);
    let second = fixture
        .repository
        .list_yard_guest_invites("yard_guest", first.next_cursor.as_ref(), 50)
        .expect("second page");
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].id, invitation_id(1));
    assert!(second.next_cursor.is_none());
    let snapshot = first
        .items
        .iter()
        .chain(&second.items)
        .map(|record| record.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(snapshot.len(), 51);
    assert_eq!(
        fixture
            .repository
            .list_yard_guest_invites("yard_guest", None, 50)
            .expect("refreshed page")
            .items[0]
            .id,
        invitation_id(52)
    );
    let mut tracker = blobyard_testkit::FixtureExecutionTracker::new("sqlite", "guest-pagination");
    tracker.record_case(
        "guest-invitation-pagination-is-deterministic-and-cursor-safe",
        &serde_json::json!({
            "resource": "guest-invitations",
            "pageSize": 50,
            "concurrentInsertAfterCursor": true
        }),
        &serde_json::json!({
            "ordering": ["createdAt-desc", "id-desc"],
            "duplicates": 0,
            "omissionsWithinSnapshot": 0
        }),
    );
    tracker.finish().expect("complete guest pagination fixture");
}

fn assert_invalid_guest_queries() {
    let fixture = Fixture::new();
    for (cursor, limit) in [
        (None, 0),
        (None, 51),
        (
            Some(YardGuestInviteCursor {
                created_at_ms: u64::MAX,
                id: INVITATION_ID.to_owned(),
            }),
            1,
        ),
        (
            Some(YardGuestInviteCursor {
                created_at_ms: 1,
                id: String::new(),
            }),
            1,
        ),
    ] {
        assert_eq!(
            fixture
                .repository
                .list_yard_guest_invites("yard_guest", cursor.as_ref(), limit),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        fixture.repository.list_yard_guest_invites("", None, 1),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .repository
            .pending_yard_guest_invite_by_token("bad", 1),
        Err(RepositoryError::InvalidInput)
    );
}

fn assert_corrupt_guest_queries_fail_closed() {
    let fixture = Fixture::new();
    fixture.create(1);
    fixture
        .repository
        .test_connection()
        .expect("connection")
        .execute(
            "UPDATE yard_access_grants SET app_roles = '{'
             WHERE id = 'yardgrant_00000000000000000000000000000001'",
            [],
        )
        .expect("corrupt invitation");
    assert_eq!(
        fixture
            .repository
            .list_yard_guest_invites("yard_guest", None, 1),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fixture
            .repository
            .pending_yard_guest_invite_by_token(&hash('c'), 2),
        Err(RepositoryError::Unavailable)
    );
}

fn assert_missing_guest_table_fails_closed() {
    let missing_table = Fixture::new();
    missing_table
        .repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             ALTER TABLE yard_guest_invitations RENAME TO unavailable_guest_invitations;",
        )
        .expect("rename invitation table");
    assert_eq!(
        missing_table
            .repository
            .list_yard_guest_invites("yard_guest", None, 1),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        missing_table
            .repository
            .pending_yard_guest_invite_by_token(&hash('c'), 2),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn exact_guest_corruption_suite_executes_generated_cases() {
    let mut tracker = blobyard_testkit::FixtureExecutionTracker::new("sqlite", "guest-corruption");
    assert_corruption(
        "PRAGMA ignore_check_constraints = ON;
         UPDATE yard_subjects SET kind = 'unknown' WHERE id = 'guest_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';",
        "corrupt-guest-subject-kind-is-inert",
        &serde_json::json!({"principalKind": "guest", "subjectKind": "unknown"}),
        &mut tracker,
    );
    assert_corruption(
        "INSERT INTO workspaces VALUES ('workspace_other', 'Other', 'other');
         UPDATE yard_subjects SET workspace_id = 'workspace_other'
         WHERE id = 'guest_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';",
        "corrupt-guest-tenant-link-is-inert",
        &serde_json::json!({
            "principalKind": "guest",
            "invitationWorkspaceMatchesSubject": false
        }),
        &mut tracker,
    );
    assert_corruption(
        "UPDATE yard_access_grants SET principal_id = 'ygi_ffffffffffffffffffffffffffffffff'
         WHERE id = 'yardgrant_00000000000000000000000000000001';",
        "corrupt-guest-grant-link-is-inert",
        &serde_json::json!({"principalKind": "guest", "grantMatchesInvitation": false}),
        &mut tracker,
    );
    assert_corruption(
        "PRAGMA ignore_check_constraints = ON;
         UPDATE yard_guest_invitations SET status = 'pending'
         WHERE id = 'ygi_00000000000000000000000000000001';",
        "corrupt-guest-lifecycle-is-inert",
        &serde_json::json!({"principalKind": "guest", "lifecycleConsistent": false}),
        &mut tracker,
    );
    tracker
        .finish()
        .expect("complete guest corruption fixtures");
}

#[test]
fn exact_guest_authority_propagation_suite_executes_generated_cases() {
    let mut tracker =
        blobyard_testkit::FixtureExecutionTracker::new("sqlite", "guest-authority-propagation");
    for (sql, fixture_id, authority, change) in [
        (
            "UPDATE yard_access_grants SET status = 'revoked', revoked_at_ms = 3
             WHERE id = 'yardgrant_00000000000000000000000000000001';",
            "revoked-guest-grant-denies-the-next-private-request",
            "grant",
            "revoked",
        ),
        (
            "UPDATE yard_access_grants SET expires_at_ms = 3
             WHERE id = 'yardgrant_00000000000000000000000000000001';",
            "expired-guest-grant-denies-the-next-private-request",
            "grant",
            "expired",
        ),
        (
            "UPDATE yard_guest_login_keys SET revoked_at_ms = 3
             WHERE id = 'yardguestkey_guest';",
            "revoked-guest-key-denies-the-next-private-request",
            "key",
            "revoked",
        ),
        (
            "UPDATE yard_guest_login_keys SET expires_at_ms = 3
             WHERE id = 'yardguestkey_guest';",
            "expired-guest-key-denies-the-next-private-request",
            "key",
            "expired",
        ),
        (
            "UPDATE yard_subjects SET revoked_at_ms = 3
             WHERE id = 'guest_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';",
            "revoked-guest-subject-denies-the-next-private-request",
            "subject",
            "revoked",
        ),
        (
            "UPDATE yard_access_policies SET visibility = 'owner'
             WHERE yard_id = 'yard_guest';",
            "guest-policy-change-denies-the-next-private-request",
            "policy",
            "owner-only",
        ),
    ] {
        assert_authority_fixture(sql, fixture_id, authority, change, &mut tracker);
    }
    tracker
        .finish()
        .expect("complete guest authority propagation fixtures");
}

fn assert_authority_fixture(
    sql: &str,
    fixture_id: &str,
    authority: &str,
    change: &str,
    tracker: &mut blobyard_testkit::FixtureExecutionTracker,
) {
    assert_authority_change(sql);
    tracker.record_case(
        fixture_id,
        &serde_json::json!({
            "authority": authority,
            "change": change,
            "surface": "private-delivery"
        }),
        &serde_json::json!({
            "admitted": false,
            "propagationMilliseconds": 0,
            "responseClass": "concealed-not-found"
        }),
    );
}

fn assert_corruption(
    sql: &str,
    fixture_id: &str,
    input: &serde_json::Value,
    tracker: &mut blobyard_testkit::FixtureExecutionTracker,
) {
    let fixture = Fixture::new();
    fixture.create_and_accept();
    let connection = rusqlite::Connection::open(&fixture.path).expect("corruption connection");
    connection
        .execute_batch("PRAGMA foreign_keys = OFF;")
        .expect("disable foreign keys");
    connection.execute_batch(sql).expect("inject corruption");
    assert_eq!(
        fixture
            .repository
            .evaluate_yard_admission("guest-yard-fixture", SUBJECT_ID, 3),
        Err(RepositoryError::NotFound)
    );
    tracker.record_case(
        fixture_id,
        input,
        &serde_json::json!({"admitted": false, "responseClass": "concealed-not-found"}),
    );
}

fn assert_authority_change(sql: &str) {
    let fixture = Fixture::new();
    fixture.create_and_accept();
    let connection = rusqlite::Connection::open(&fixture.path).expect("authority connection");
    connection.execute_batch(sql).expect("mutate one authority");
    assert_eq!(
        fixture
            .repository
            .evaluate_yard_admission("guest-yard-fixture", SUBJECT_ID, 3),
        Err(RepositoryError::NotFound)
    );
}
