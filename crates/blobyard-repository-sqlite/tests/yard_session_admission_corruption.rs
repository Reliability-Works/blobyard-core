#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Fail-closed admission coverage for lifecycle and membership corruption.

use blobyard_contract::{
    LifecycleRepository, NewYardContinuation, NewYardSession, RepositoryError, WebYardRepository,
    YARD_EXCHANGE_CODE_LIFETIME_MS, YARD_SESSION_LIFETIME_MS, YardSessionAuditContext,
    YardSessionRepository,
};
use blobyard_testkit::FixtureExecutionTracker;
use rusqlite::Connection;

#[path = "support/yard_session_corruption.rs"]
mod corruption_support;
use corruption_support::{CORRUPTIONS, Corruption, Fixture};

#[test]
fn corrupt_group_lifecycle_is_concealed_at_every_admission_boundary() {
    let mut tracker = FixtureExecutionTracker::new("sqlite", "admission-corruption");
    for corruption in CORRUPTIONS {
        assert_issue_conceals(corruption);
        assert_exchange_conceals_and_rolls_back(corruption);
        assert_delivery_conceals(corruption);
        assert_fixture_case(corruption, &mut tracker);
    }
    tracker.finish().expect("complete fixtures");
}

#[test]
fn valid_direct_grant_survives_over_limit_group_corruption() {
    for corruption in [
        Corruption::OverLimitActiveGroupGrants,
        Corruption::OverLimitMembershipRows,
    ] {
        let fixture = Fixture::new();
        Connection::open(&fixture.path)
            .expect("direct grant connection")
            .execute_batch(
                "INSERT INTO yard_access_grants VALUES
                   ('grant_direct_fixture', 'yard_fixture', NULL, 'user', 'user_fixture', '[]',
                    'active', 2, 'fixture', NULL, NULL);",
            )
            .expect("direct grant");
        fixture.set_corruption(corruption, true);
        let continuation = continuation();
        fixture
            .repository
            .issue_yard_exchange_code(&continuation)
            .expect("direct issue");
        let session = session();
        fixture
            .repository
            .exchange_yard_session_code(
                &continuation.code_hash,
                &continuation.host_label,
                &session,
                &audit(),
                11,
            )
            .expect("direct exchange");
        fixture
            .repository
            .yard_file_by_host(
                &continuation.host_label,
                "asset.js",
                Some(&session.token_hash),
                12,
            )
            .expect("direct delivery");
    }
}

fn assert_issue_conceals(corruption: Corruption) {
    let fixture = Fixture::new();
    let continuation = continuation();
    fixture.set_corruption(corruption, true);
    assert_eq!(
        fixture.repository.issue_yard_exchange_code(&continuation),
        Err(RepositoryError::NotFound),
        "{corruption:?}"
    );
    fixture.set_corruption(corruption, false);
    fixture
        .repository
        .issue_yard_exchange_code(&continuation)
        .expect("failed issue must not persist a continuation");
}

fn assert_exchange_conceals_and_rolls_back(corruption: Corruption) {
    let fixture = Fixture::new();
    let continuation = continuation();
    fixture
        .repository
        .issue_yard_exchange_code(&continuation)
        .expect("issue");
    let session = session();
    let audit = audit();
    fixture.set_corruption(corruption, true);
    assert_eq!(
        fixture.repository.exchange_yard_session_code(
            &continuation.code_hash,
            &continuation.host_label,
            &session,
            &audit,
            11,
        ),
        Err(RepositoryError::NotFound),
        "{corruption:?}"
    );
    fixture.set_corruption(corruption, false);
    fixture
        .repository
        .exchange_yard_session_code(
            &continuation.code_hash,
            &continuation.host_label,
            &session,
            &audit,
            11,
        )
        .expect("failed exchange must not consume the code");
    assert_eq!(
        fixture
            .repository
            .list_audit("workspace_fixture", None, 50)
            .expect("audits")
            .items
            .iter()
            .filter(|event| event.id == audit.id)
            .count(),
        1
    );
}

fn assert_delivery_conceals(corruption: Corruption) {
    let fixture = Fixture::new();
    let continuation = continuation();
    fixture
        .repository
        .issue_yard_exchange_code(&continuation)
        .expect("issue");
    let session = session();
    fixture
        .repository
        .exchange_yard_session_code(
            &continuation.code_hash,
            &continuation.host_label,
            &session,
            &audit(),
            11,
        )
        .expect("exchange");
    fixture.set_corruption(corruption, true);
    assert_eq!(
        fixture.repository.yard_file_by_host(
            &continuation.host_label,
            "asset.js",
            Some(&session.token_hash),
            12,
        ),
        Err(RepositoryError::NotFound),
        "{corruption:?}"
    );
    fixture.set_corruption(corruption, false);
    fixture
        .repository
        .yard_file_by_host(
            &continuation.host_label,
            "asset.js",
            Some(&session.token_hash),
            12,
        )
        .expect("restored delivery");
}

fn assert_fixture_case(corruption: Corruption, tracker: &mut FixtureExecutionTracker) {
    if let Some((id, input, expected)) = fixture_case(corruption) {
        tracker.record_case(id, &input, &expected);
    }
}

type FixtureCase = (&'static str, serde_json::Value, serde_json::Value);

fn fixture_case(corruption: Corruption) -> Option<FixtureCase> {
    match corruption {
        Corruption::ActiveGrantWithRevocation
        | Corruption::ActiveGroupWithDeactivation
        | Corruption::CrossWorkspaceGroup => lifecycle_fixture_case(corruption),
        Corruption::IncorrectMemberCount
        | Corruption::NonmatchingEnvironment
        | Corruption::SameNameForeignGroup
        | Corruption::UnresolvedGroup => admission_fixture_case(corruption),
        Corruption::InvalidMembershipTimestamp
        | Corruption::OverLimitActiveGroupGrants
        | Corruption::OverLimitMembershipRows => None,
    }
}

fn lifecycle_fixture_case(corruption: Corruption) -> Option<FixtureCase> {
    let fixture = match corruption {
        Corruption::ActiveGrantWithRevocation => denied_fixture(
            "active-grant-with-revocation-timestamp-is-inert",
            serde_json::json!({
                "principalKind": "group",
                "grantStatus": "active",
                "revokedAtPresent": true
            }),
        ),
        Corruption::ActiveGroupWithDeactivation => denied_fixture(
            "active-group-with-deactivation-timestamp-is-inert",
            serde_json::json!({
                "principalKind": "group",
                "groupStatus": "active",
                "deactivatedAtPresent": true
            }),
        ),
        Corruption::CrossWorkspaceGroup => denied_fixture(
            "cross-workspace-group-id-never-admits",
            serde_json::json!({
                "principalKind": "group",
                "grantWorkspace": "default",
                "groupWorkspace": "other",
                "membershipStatus": "active"
            }),
        ),
        _ => return None,
    };
    Some(fixture)
}

fn admission_fixture_case(corruption: Corruption) -> Option<FixtureCase> {
    let fixture = match corruption {
        Corruption::IncorrectMemberCount => denied_fixture(
            "corrupt-group-membership-cardinality-is-inert",
            serde_json::json!({
                "principalKind": "group",
                "memberCountMatchesMembershipRows": false
            }),
        ),
        Corruption::NonmatchingEnvironment => denied_fixture(
            "nonmatching-environment-group-grant-denies",
            serde_json::json!({
                "principalKind": "group",
                "grantEnvironment": "staging",
                "selectedEnvironment": "production"
            }),
        ),
        Corruption::SameNameForeignGroup => denied_fixture(
            "same-name-foreign-group-never-admits",
            serde_json::json!({
                "principalKind": "group",
                "selectedGroupName": "Readers",
                "membershipGroupName": "Readers",
                "sameGroupId": false
            }),
        ),
        Corruption::UnresolvedGroup => denied_fixture(
            "unresolved-legacy-group-grant-remains-inert",
            serde_json::json!({
                "grantStatus": "active",
                "groupStatus": "unresolved",
                "membershipStatus": "absent",
                "principalKind": "group",
                "surface": "selected-yard"
            }),
        ),
        _ => return None,
    };
    Some(fixture)
}

fn denied_fixture(id: &'static str, input: serde_json::Value) -> FixtureCase {
    (
        id,
        input,
        serde_json::json!({
            "admitted": false,
            "responseClass": "concealed-not-found"
        }),
    )
}

fn continuation() -> NewYardContinuation {
    NewYardContinuation {
        id: "continuation_corruption".to_owned(),
        continuation_hash: hash('c'),
        code_hash: hash('e'),
        yard_id: "yard_fixture".to_owned(),
        environment_id: "environment_fixture".to_owned(),
        host_label: "docs-fixture".to_owned(),
        user_id: "user_fixture".to_owned(),
        return_path: "/".to_owned(),
        created_at_ms: 10,
        expires_at_ms: 10 + YARD_EXCHANGE_CODE_LIFETIME_MS,
    }
}

fn session() -> NewYardSession {
    NewYardSession {
        id: "session_corruption".to_owned(),
        token_hash: hash('f'),
        created_at_ms: 11,
        expires_at_ms: 11 + YARD_SESSION_LIFETIME_MS,
    }
}

fn audit() -> YardSessionAuditContext {
    YardSessionAuditContext {
        id: "audit_corruption".to_owned(),
        request_id: "request_corruption".to_owned(),
    }
}

fn hash(value: char) -> String {
    value.to_string().repeat(64)
}
