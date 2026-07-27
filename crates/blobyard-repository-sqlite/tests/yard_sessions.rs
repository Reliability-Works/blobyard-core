#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Yard session validation and housekeeping boundary coverage.

use blobyard_contract::{
    NewYardSession, RepositoryError, YARD_SESSION_LIFETIME_MS, YardSessionAuditContext,
    YardSessionRepository,
};
use blobyard_repository_sqlite::SqliteRepository;
use rusqlite::{Connection, params};

include!("support/yard_sessions.rs");

const NOW_MS: u64 = 3_000_000_000;
const CONTINUATION_BEFORE: u64 = NOW_MS - 86_400_000;
const SESSION_BEFORE: u64 = NOW_MS - 2_592_000_000;

#[test]
fn malformed_admission_and_continuation_inputs_fail_before_lookup() {
    let temporary = tempfile::tempdir().expect("temporary");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");

    assert_eq!(
        repository.evaluate_yard_admission("invalid", "user_fixture", 10),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.issue_yard_exchange_code(&continuation("https://example.test")),
        Err(RepositoryError::InvalidInput)
    );
    let mut malformed_hash = continuation("/");
    malformed_hash.continuation_hash = "not-a-hash".to_owned();
    assert_eq!(
        repository.issue_yard_exchange_code(&malformed_hash),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn malformed_session_and_housekeeping_inputs_fail_before_lookup() {
    let temporary = tempfile::tempdir().expect("temporary");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");

    assert_eq!(
        repository.exchange_yard_session_code(
            &hash('c'),
            "docs-fixture",
            &NewYardSession {
                id: "session_fixture".to_owned(),
                token_hash: hash('d'),
                created_at_ms: 20,
                expires_at_ms: 21,
            },
            &YardSessionAuditContext {
                id: "audit_fixture".to_owned(),
                request_id: "request_fixture".to_owned(),
            },
            20,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.exchange_yard_session_code(
            &hash('c'),
            "docs-fixture",
            &NewYardSession {
                id: "session_fixture".to_owned(),
                token_hash: hash('d'),
                created_at_ms: 20,
                expires_at_ms: 20 + YARD_SESSION_LIFETIME_MS,
            },
            &YardSessionAuditContext {
                id: String::new(),
                request_id: "request_fixture".to_owned(),
            },
            20,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.revoke_yard_session_by_token(&hash('e'), "invalid", 20),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.list_yard_sessions(""),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.purge_yard_session_history(u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn malformed_persisted_session_rows_fail_closed() {
    let (_temporary, repository) = corrupted_repository(
        "INSERT INTO local_users (id, workspace_id, display_name, status, created_at_ms)
             VALUES ('user_fixture', 'workspace_fixture', 'Fixture user', 'active', 1);
             INSERT INTO yard_subjects
               (id, kind, workspace_id, local_user_id, created_at_ms)
             VALUES ('user_fixture', 'member', 'workspace_fixture', 'user_fixture', 1);
             INSERT INTO yard_sessions
               (id, token_hash, yard_id, environment_id, host_label, subject_id,
                created_at_ms, expires_at_ms)
             VALUES
               ('session_corrupt',
                'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
                'yard_fixture', 'environment_fixture', 'docs-fixture', 'user_fixture',
                -1, 100);",
    );

    assert_eq!(
        repository.list_yard_sessions("yard_fixture"),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn malformed_persisted_continuation_rows_fail_closed() {
    let (_temporary, repository) = corrupted_repository(
        "INSERT INTO yard_continuations
           (id, continuation_hash, code_hash, yard_id, environment_id, host_label, subject_id,
            return_path, created_at_ms, expires_at_ms)
         VALUES
           ('continuation_corrupt',
            'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
            'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
            'yard_fixture', 'environment_fixture', 'docs-fixture', 'user_fixture',
            '/', -1, 100);",
    );

    assert_eq!(
        repository.exchange_yard_session_code(
            &hash('b'),
            "docs-fixture",
            &NewYardSession {
                id: "session_fixture".to_owned(),
                token_hash: hash('d'),
                created_at_ms: 1,
                expires_at_ms: 1 + YARD_SESSION_LIFETIME_MS,
            },
            &YardSessionAuditContext {
                id: "audit_fixture".to_owned(),
                request_id: "request_fixture".to_owned(),
            },
            1,
        ),
        Err(RepositoryError::Unavailable)
    );
}

fn corrupted_repository(sql: &str) -> (tempfile::TempDir, SqliteRepository) {
    let temporary = tempfile::tempdir().expect("temporary");
    let path = temporary.path().join("metadata.sqlite3");
    let repository = SqliteRepository::open(&path).expect("repository");
    Connection::open(path)
        .expect("fixture connection")
        .execute_batch(&format!(
            "PRAGMA foreign_keys = OFF;
             PRAGMA ignore_check_constraints = ON;
             {sql}"
        ))
        .expect("corrupt fixture");
    (temporary, repository)
}

#[test]
fn housekeeping_preserves_exact_boundaries_and_removes_older_history() {
    let temporary = tempfile::tempdir().expect("temporary");
    let path = temporary.path().join("metadata.sqlite3");
    let repository = SqliteRepository::open(&path).expect("repository");
    let connection = Connection::open(&path).expect("fixture connection");
    connection
        .execute_batch("PRAGMA foreign_keys = OFF;")
        .expect("disable fixture foreign keys");
    seed_housekeeping(&connection);
    drop(connection);

    repository
        .purge_yard_session_history(NOW_MS)
        .expect("housekeeping");

    let connection = Connection::open(path).expect("inspection connection");
    assert_eq!(
        ids(&connection, "yard_continuations"),
        vec!["continuation_at_boundary"]
    );
    assert_eq!(
        ids(&connection, "yard_sessions"),
        vec![
            "session_active",
            "session_expired_at_boundary",
            "session_revoked_at_boundary",
        ]
    );
}

fn seed_housekeeping(connection: &Connection) {
    insert_continuation(
        connection,
        "continuation_at_boundary",
        'a',
        CONTINUATION_BEFORE,
    );
    insert_continuation(
        connection,
        "continuation_before_boundary",
        'b',
        CONTINUATION_BEFORE - 1,
    );
    insert_session(
        connection,
        "session_expired_at_boundary",
        'c',
        SESSION_BEFORE,
        None,
    );
    insert_session(
        connection,
        "session_expired_before_boundary",
        'd',
        SESSION_BEFORE - 1,
        None,
    );
    insert_session(
        connection,
        "session_revoked_at_boundary",
        'e',
        NOW_MS + 1,
        Some(SESSION_BEFORE),
    );
    insert_session(
        connection,
        "session_revoked_before_boundary",
        'f',
        NOW_MS + 1,
        Some(SESSION_BEFORE - 1),
    );
    insert_session(connection, "session_active", '7', NOW_MS + 1, None);
}

fn insert_continuation(connection: &Connection, id: &str, marker: char, expires_at_ms: u64) {
    connection
        .execute(
            "INSERT INTO yard_continuations
             (id, continuation_hash, code_hash, yard_id, environment_id, host_label, subject_id,
              return_path, created_at_ms, expires_at_ms)
             VALUES
             (?1, ?2, ?3, 'yard_fixture', 'environment_fixture', 'docs-fixture', 'user_fixture',
              '/', ?4, ?5)",
            params![
                id,
                hash(marker),
                hash(char::from_u32(u32::from(marker) + 1).expect("next marker")),
                i64::try_from(expires_at_ms - 1).expect("created time"),
                i64::try_from(expires_at_ms).expect("expiry time"),
            ],
        )
        .expect("continuation fixture");
}

fn insert_session(
    connection: &Connection,
    id: &str,
    marker: char,
    expires_at_ms: u64,
    revoked_at_ms: Option<u64>,
) {
    connection
        .execute(
            "INSERT INTO yard_sessions
             (id, token_hash, yard_id, environment_id, host_label, subject_id,
              created_at_ms, expires_at_ms, revoked_at_ms)
             VALUES
             (?1, ?2, 'yard_fixture', 'environment_fixture', 'docs-fixture', 'user_fixture',
              1, ?3, ?4)",
            params![
                id,
                hash(marker),
                i64::try_from(expires_at_ms).expect("expiry time"),
                revoked_at_ms.map(|value| i64::try_from(value).expect("revocation time")),
            ],
        )
        .expect("session fixture");
}

fn ids(connection: &Connection, table: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("SELECT id FROM {table} ORDER BY id"))
        .expect("query");
    statement
        .query_map([], |row| row.get(0))
        .expect("rows")
        .map(|row| row.expect("id"))
        .collect()
}
