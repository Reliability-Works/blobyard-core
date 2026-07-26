#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{require_admission, require_session_yard, session_times};
use blobyard_contract::{
    AuditValue, NewAuditEvent, RepositoryError, YardAdmission, YardSessionRecord,
};
use rusqlite::Connection;

fn admission() -> YardAdmission {
    YardAdmission {
        yard_id: "yard".to_owned(),
        environment_id: "environment".to_owned(),
        workspace_id: "workspace".to_owned(),
    }
}

fn session() -> YardSessionRecord {
    YardSessionRecord {
        id: "session".to_owned(),
        token_hash: "a".repeat(64),
        yard_id: "yard".to_owned(),
        environment_id: "environment".to_owned(),
        host_label: "docs-fixture".to_owned(),
        user_id: "user".to_owned(),
        created_at_ms: 1,
        expires_at_ms: 2,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

#[test]
fn persisted_identifiers_must_match_current_admission() {
    assert_eq!(
        require_admission(&admission(), "yard", "environment"),
        Ok(())
    );
    assert_eq!(
        require_admission(&admission(), "other", "environment"),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        require_admission(&admission(), "yard", "other"),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(require_session_yard(&session(), "yard"), Ok(()));
    assert_eq!(
        require_session_yard(&session(), "other"),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn session_times_reject_values_outside_sqlite_range() {
    assert_eq!(session_times(&session()), Ok((1, 2)));
    let mut malformed = session();
    malformed.created_at_ms = (i64::MAX as u64) + 1;
    assert_eq!(
        session_times(&malformed),
        Err(RepositoryError::InvalidInput)
    );
    malformed.created_at_ms = 1;
    malformed.expires_at_ms = (i64::MAX as u64) + 1;
    assert_eq!(
        session_times(&malformed),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn listing_maps_parameter_binding_failures() {
    let connection = Connection::open_in_memory().expect("connection");
    let mut statement = connection.prepare("SELECT 1").expect("statement");
    assert_eq!(
        super::list(&mut statement, "yard"),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn revocation_conceals_missing_and_foreign_sessions_and_rejects_invalid_audit() {
    let temporary = tempfile::tempdir().expect("temporary");
    let repository =
        super::super::SqliteRepository::open(&temporary.path().join("metadata.sqlite3"))
            .expect("repository");
    let mut connection = repository.test_connection().expect("connection");
    connection
        .pragma_update(None, "foreign_keys", "OFF")
        .expect("foreign keys");
    connection
        .execute_batch(
            "INSERT INTO web_yards (id, workspace_id, project_id, name, host_label, status, created_at_ms, updated_at_ms) VALUES ('yard_fixture', 'workspace_fixture', 'project_fixture', 'docs', 'docs-fixture', 'active', 1, 1);
             INSERT INTO yard_sessions (id, token_hash, yard_id, environment_id, host_label, user_id, created_at_ms, expires_at_ms) VALUES ('session_fixture', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'yard_fixture', 'environment_fixture', 'docs-fixture', 'user_fixture', 1, 10);",
        )
        .expect("fixture");
    let transaction = connection.transaction().expect("transaction");
    let event = revoked_event();
    assert_eq!(
        super::revoke(&transaction, "yard_fixture", "session_missing", 2, &event,),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        super::revoke(&transaction, "yard_other", "session_fixture", 2, &event,),
        Err(RepositoryError::NotFound)
    );
    let mut invalid = event;
    invalid.action = "wrong.action".to_owned();
    assert_eq!(
        super::revoke(&transaction, "yard_fixture", "session_fixture", 2, &invalid,),
        Err(RepositoryError::InvalidInput)
    );
    drop(transaction);
    drop(connection);
}

fn revoked_event() -> NewAuditEvent {
    NewAuditEvent {
        id: "audit_session".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "user_fixture".to_owned(),
        action: "yard.session_revoked".to_owned(),
        request_id: "request_session".to_owned(),
        target_type: "yard_session".to_owned(),
        metadata: vec![
            (
                "sessionId".to_owned(),
                AuditValue::String("session_fixture".to_owned()),
            ),
            (
                "yardId".to_owned(),
                AuditValue::String("yard_fixture".to_owned()),
            ),
        ],
        created_at_ms: 2,
    }
}
