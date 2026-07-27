#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{admission, admitted_session_id, session_id};
use blobyard_contract::{RepositoryError, YardAdmission};
use rusqlite::Connection;

#[test]
fn admission_row_rejects_each_non_text_column() {
    let connection = Connection::open_in_memory().expect("connection");
    let base = ["'yard'", "'environment'", "'workspace'"];
    for index in 0..base.len() {
        let mut values = base;
        values[index] = "X'00'";
        let query = format!("SELECT {}", values.join(", "));
        assert!(connection.query_row(&query, [], admission).is_err());
    }
}

#[test]
fn session_lookup_rejects_each_non_text_identity_column() {
    for column in ["id", "subject_id", "environment_id"] {
        let connection = Connection::open_in_memory().expect("connection");
        connection
            .execute_batch(
                "CREATE TABLE yard_sessions (
                   id, token_hash, yard_id, environment_id, host_label, subject_id,
                   expires_at_ms, revoked_at_ms
                 );
                 INSERT INTO yard_sessions VALUES (
                   'session', 'token', 'yard', 'environment', 'host', 'subject', 10, NULL
                 );",
            )
            .expect("session fixture");
        connection
            .execute(&format!("UPDATE yard_sessions SET {column} = X'00'"), [])
            .expect("corrupt session column");
        assert_eq!(
            session_id(&connection, "token", "host", "yard", "selected", 1),
            Err(RepositoryError::Unavailable),
            "column {column}"
        );
    }
}

#[test]
fn a_session_is_returned_only_for_its_current_admission() {
    let admission = YardAdmission {
        yard_id: "yard".to_owned(),
        environment_id: "environment".to_owned(),
        workspace_id: "workspace".to_owned(),
    };
    assert_eq!(
        admitted_session_id("session".to_owned(), "environment", &admission, "yard"),
        Some("session".to_owned())
    );
    assert_eq!(
        admitted_session_id("session".to_owned(), "other", &admission, "yard"),
        None
    );
    assert_eq!(
        admitted_session_id("session".to_owned(), "environment", &admission, "other"),
        None
    );
}
