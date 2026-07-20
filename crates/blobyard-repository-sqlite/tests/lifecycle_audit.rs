#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Audit validation and corrupt persistence coverage.

/// Shared lifecycle repository fixtures.
#[path = "lifecycle_support/mod.rs"]
pub mod support;

use blobyard_contract::{AuditValue, LifecycleRepository, NewAuditEvent, RepositoryError};
use support::{Fixture, event};

#[test]
fn audit_round_trips_safe_values_and_rejects_invalid_records() {
    let fixture = Fixture::new();
    let safe_event = NewAuditEvent {
        metadata: vec![
            ("text".to_owned(), AuditValue::String("safe".to_owned())),
            ("number".to_owned(), AuditValue::Number(7)),
            ("bool".to_owned(), AuditValue::Boolean(false)),
            ("null".to_owned(), AuditValue::Null),
        ],
        ..event("audit_one", "fixture.recorded", "request_one", 1)
    };
    fixture
        .repository
        .record_audit(&safe_event)
        .expect("audit event");
    let page = fixture
        .repository
        .list_audit("workspace_fixture", None, 100)
        .expect("audit page");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].metadata.len(), 4);
    assert_eq!(page.next_before, None);
    assert_audit_query_validation(&fixture);
    assert_audit_record_validation(&fixture, &safe_event);
}

fn assert_audit_query_validation(fixture: &Fixture) {
    for limit in [0, 101] {
        assert_eq!(
            fixture
                .repository
                .list_audit("workspace_fixture", None, limit),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        fixture
            .repository
            .list_audit("workspace_fixture", Some(u64::MAX), 1),
        Err(RepositoryError::InvalidInput)
    );
}

fn assert_audit_record_validation(fixture: &Fixture, safe_event: &NewAuditEvent) {
    let mut duplicate = event("audit_duplicate", "fixture.recorded", "request_two", 2);
    duplicate.metadata = vec![
        ("same".to_owned(), AuditValue::Boolean(true)),
        ("same".to_owned(), AuditValue::Boolean(false)),
    ];
    assert_eq!(
        fixture.repository.record_audit(&duplicate),
        Err(RepositoryError::InvalidInput)
    );
    let mut empty = event("audit_empty", "fixture.recorded", "request_three", 3);
    empty.metadata = vec![("value".to_owned(), AuditValue::String(String::new()))];
    assert_eq!(
        fixture.repository.record_audit(&empty),
        Err(RepositoryError::InvalidInput)
    );
    let overflow = event(
        "audit_overflow",
        "fixture.recorded",
        "request_four",
        u64::MAX,
    );
    assert_eq!(
        fixture.repository.record_audit(&overflow),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.record_audit(safe_event),
        Err(RepositoryError::Conflict)
    );
}

#[test]
fn audit_listing_fails_closed_for_unsafe_persisted_metadata() {
    for (id, metadata) in [
        ("audit_array", r#"{"unsafe":[]}"#),
        ("audit_negative", r#"{"unsafe":-1}"#),
    ] {
        let fixture = Fixture::new();
        let connection = rusqlite::Connection::open(&fixture.path).expect("fixture database");
        connection
            .execute(
                "INSERT INTO audit_events (id, workspace_id, actor, action, request_id, target_type, metadata_json, created_at_ms) VALUES (?1, 'workspace_fixture', 'token_fixture', 'fixture.recorded', 'request_fixture', 'fixture', ?2, 1)",
                rusqlite::params![id, metadata],
            )
            .expect("corrupt audit fixture");
        drop(connection);
        assert_eq!(
            fixture.repository.list_audit("workspace_fixture", None, 1),
            Err(RepositoryError::Unavailable)
        );
    }
}
