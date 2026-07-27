#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{insert_session, purge, require_admission, require_session_yard};
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
fn listing_maps_parameter_binding_failures() {
    let connection = Connection::open_in_memory().expect("connection");
    let mut statement = connection.prepare("SELECT 1").expect("statement");
    assert_eq!(
        super::list(&mut statement, "yard"),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn purge_saturates_history_cutoffs_at_the_epoch() {
    let mut connection = Connection::open_in_memory().expect("connection");
    connection
        .execute_batch(
            "CREATE TABLE yard_continuations (expires_at_ms INTEGER NOT NULL);
             CREATE TABLE yard_sessions (
               expires_at_ms INTEGER NOT NULL,
               revoked_at_ms INTEGER
             );",
        )
        .expect("schema");
    let transaction = connection.transaction().expect("transaction");
    assert_eq!(purge(&transaction, 0), Ok(()));
}

#[test]
fn session_insert_maps_statement_failures_and_persists_valid_records() {
    let mut connection = Connection::open_in_memory().expect("connection");
    let transaction = connection.transaction().expect("transaction");
    let mut invalid_time = session();
    invalid_time.expires_at_ms = u64::MAX;
    assert_eq!(
        insert_session(&transaction, &invalid_time, 1),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        insert_session(&transaction, &session(), 1),
        Err(RepositoryError::Unavailable)
    );
    transaction
        .execute_batch(
            "CREATE TABLE yard_subjects (
               id TEXT PRIMARY KEY,
               revoked_at_ms INTEGER
             );
             CREATE TABLE yard_sessions (
               id TEXT PRIMARY KEY,
               token_hash TEXT NOT NULL,
               yard_id TEXT NOT NULL,
               environment_id TEXT NOT NULL,
               host_label TEXT NOT NULL,
               subject_id TEXT NOT NULL,
               created_at_ms INTEGER NOT NULL,
               expires_at_ms INTEGER NOT NULL,
               last_used_at_ms INTEGER,
               revoked_at_ms INTEGER
             );
             INSERT INTO yard_subjects (id, revoked_at_ms) VALUES ('user', NULL);",
        )
        .expect("schema");
    let mut missing_subject = session();
    "missing".clone_into(&mut missing_subject.user_id);
    assert_eq!(
        insert_session(&transaction, &missing_subject, 1),
        Err(RepositoryError::Conflict)
    );
    assert_eq!(insert_session(&transaction, &session(), 1), Ok(()));
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
             INSERT INTO yard_sessions
               (id, token_hash, yard_id, environment_id, host_label, subject_id,
                created_at_ms, expires_at_ms)
             VALUES
               ('session_fixture',
                'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                'yard_fixture', 'environment_fixture', 'docs-fixture', 'user_fixture',
                1, 10);",
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
