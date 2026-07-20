#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{LocalApiTokenRecord, LocalCliSessionRecord, insert_session, query_cli_sessions};
use crate::SqliteRepository;
use blobyard_contract::{
    CredentialRepository, MetadataRepository, RepositoryError, WorkspaceRecord,
};
use blobyard_core::Slug;
use rusqlite::Connection;

#[test]
fn session_insert_rejects_each_timestamp_beyond_sqlite_range() {
    let connection = Connection::open_in_memory().expect("database");
    connection
        .execute_batch(
            "CREATE TABLE cli_sessions (id TEXT, token_id TEXT, workspace_id TEXT, name TEXT, platform TEXT, version TEXT, created_at_ms INTEGER, last_used_at_ms INTEGER, revoked_at_ms INTEGER);",
        )
        .expect("session table");
    let mut created = session(&token());
    created.created_at_ms = u64::MAX;
    let mut used = session(&token());
    used.last_used_at_ms = Some(u64::MAX);
    let mut revoked = session(&token());
    revoked.revoked_at_ms = Some(u64::MAX);
    for candidate in [created, used, revoked] {
        assert_eq!(
            insert_session(&connection, &candidate),
            Err(RepositoryError::InvalidInput)
        );
    }
}

#[test]
fn session_queries_reject_each_corrupt_persisted_field() {
    let (_temporary, repository, token) = seeded();
    corrupt(&repository, "platform");
    assert_eq!(
        repository.list_cli_sessions(&token.workspace_id),
        Err(RepositoryError::Unavailable)
    );

    for column in ["token_id", "revoked_at_ms"] {
        let (_temporary, repository, token) = seeded();
        corrupt(&repository, column);
        assert_eq!(
            repository.revoke_cli_session(
                "session_fixture",
                &token.workspace_id,
                2,
                &revoke_event(&token.workspace_id),
            ),
            Err(RepositoryError::Unavailable)
        );
    }
}

#[test]
fn session_query_maps_parameter_binding_failures() {
    let connection = Connection::open_in_memory().expect("database");
    let mut statement = connection.prepare("SELECT 1").expect("statement");

    assert_eq!(
        query_cli_sessions(&mut statement, "workspace_fixture"),
        Err(RepositoryError::Unavailable)
    );
}

fn seeded() -> (tempfile::TempDir, SqliteRepository, LocalApiTokenRecord) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");
    let token = token();
    repository
        .create_workspace(&WorkspaceRecord {
            id: token.workspace_id.clone(),
            name: "Fixture".to_owned(),
            slug: Slug::new("fixture").expect("workspace slug"),
        })
        .expect("workspace");
    repository
        .install_bootstrap(&checksum('b'))
        .expect("bootstrap");
    repository
        .exchange_bootstrap(&checksum('b'), &token, &session(&token))
        .expect("session");
    (temporary, repository, token)
}

fn corrupt(repository: &SqliteRepository, column: &str) {
    let connection = repository.test_connection().expect("connection");
    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             ALTER TABLE cli_sessions RENAME TO valid_cli_sessions;
             CREATE TABLE cli_sessions (id, token_id, workspace_id, name, platform, version, created_at_ms, last_used_at_ms, revoked_at_ms);
             INSERT INTO cli_sessions SELECT id, token_id, workspace_id, name, platform, version, created_at_ms, last_used_at_ms, revoked_at_ms FROM valid_cli_sessions;",
        )
        .expect("replace strict table with corruption fixture");
    connection
        .execute(&format!("UPDATE cli_sessions SET {column} = x'00'"), [])
        .expect("corrupt session column");
    drop(connection);
}

fn token() -> LocalApiTokenRecord {
    LocalApiTokenRecord {
        id: "token_fixture".to_owned(),
        name: "Fixture".to_owned(),
        token_prefix: "byd_pat_fixture".to_owned(),
        secret_hash: checksum('c'),
        scopes: vec!["object:read".to_owned()],
        workspace_id: "workspace_fixture".to_owned(),
        project_id: None,
        created_at_ms: 1,
        expires_at_ms: 100,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

fn session(token: &LocalApiTokenRecord) -> LocalCliSessionRecord {
    blobyard_testkit::cli_session_record(token, "0.1.12")
}

fn revoke_event(workspace_id: &str) -> blobyard_contract::NewAuditEvent {
    blobyard_testkit::cli_session_revoked_event(workspace_id, "session_fixture", 2)
}

fn checksum(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}
