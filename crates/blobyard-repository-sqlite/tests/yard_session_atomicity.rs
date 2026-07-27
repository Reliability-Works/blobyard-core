#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Concurrent Yard exchange atomicity coverage.

use blobyard_contract::{
    LifecycleRepository, NewYardContinuation, NewYardSession, RepositoryError,
    YARD_EXCHANGE_CODE_LIFETIME_MS, YARD_SESSION_LIFETIME_MS, YardSessionAuditContext,
    YardSessionRepository,
};
use blobyard_repository_sqlite::SqliteRepository;
use rusqlite::Connection;
use std::sync::{Arc, Barrier};

fn hash(value: char) -> String {
    value.to_string().repeat(64)
}

#[test]
fn concurrent_code_exchange_mints_exactly_one_session_and_audit_event() {
    let temporary = tempfile::tempdir().expect("temporary");
    let path = temporary.path().join("metadata.sqlite3");
    let repository = SqliteRepository::open(&path).expect("repository");
    seed_admission(&path);
    repository
        .issue_yard_exchange_code(&NewYardContinuation {
            id: "continuation_fixture".to_owned(),
            continuation_hash: hash('a'),
            code_hash: hash('b'),
            yard_id: "yard_fixture".to_owned(),
            environment_id: "environment_fixture".to_owned(),
            host_label: "docs-fixture".to_owned(),
            user_id: "user_fixture".to_owned(),
            return_path: "/".to_owned(),
            created_at_ms: 10,
            expires_at_ms: 10 + YARD_EXCHANGE_CODE_LIFETIME_MS,
        })
        .expect("continuation");

    assert_eq!(exchange_counts(&repository), (1, 1));
    assert_eq!(
        repository
            .list_yard_sessions("yard_fixture")
            .expect("sessions")
            .len(),
        1
    );
    assert_eq!(
        repository
            .list_audit("workspace_fixture", None, 100)
            .expect("audit")
            .items
            .iter()
            .filter(|event| event.action == "yard.session_issued")
            .count(),
        1
    );
}

fn exchange_counts(repository: &SqliteRepository) -> (usize, usize) {
    let barrier = Arc::new(Barrier::new(3));
    let results = std::thread::scope(|scope| {
        let handles = [(1, 'c'), (2, 'd')].map(|(index, marker)| {
            let barrier = Arc::clone(&barrier);
            scope.spawn(move || {
                barrier.wait();
                repository.exchange_yard_session_code(
                    &hash('b'),
                    "docs-fixture",
                    &NewYardSession {
                        id: format!("session_{index}"),
                        token_hash: hash(marker),
                        created_at_ms: 11,
                        expires_at_ms: 11 + YARD_SESSION_LIFETIME_MS,
                    },
                    &YardSessionAuditContext {
                        id: format!("audit_{index}"),
                        request_id: format!("request_{index}"),
                    },
                    11,
                )
            })
        });
        barrier.wait();
        handles.map(|handle| handle.join().expect("exchange thread"))
    });
    (
        results.iter().filter(|result| result.is_ok()).count(),
        results
            .iter()
            .filter(|result| matches!(result, Err(RepositoryError::NotFound)))
            .count(),
    )
}

fn seed_admission(path: &std::path::Path) {
    Connection::open(path)
        .expect("fixture connection")
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             INSERT INTO workspaces (id, name, slug)
             VALUES ('workspace_fixture', 'Fixture', 'fixture');
             INSERT INTO projects (id, workspace_id, name, slug)
             VALUES ('project_fixture', 'workspace_fixture', 'Fixture', 'fixture');
             INSERT INTO local_users (id, workspace_id, display_name, status, created_at_ms)
             VALUES ('user_fixture', 'workspace_fixture', 'Fixture user', 'active', 1);
             INSERT INTO yard_subjects (id, kind, workspace_id, local_user_id, created_at_ms)
             VALUES ('user_fixture', 'member', 'workspace_fixture', 'user_fixture', 1);
             INSERT INTO web_yards (id, workspace_id, project_id, name, host_label, status, created_at_ms, updated_at_ms)
             VALUES ('yard_fixture', 'workspace_fixture', 'project_fixture', 'docs', 'docs-fixture', 'active', 1, 1);
             INSERT INTO yard_environments (id, yard_id, name, kind, status, created_at_ms, updated_at_ms)
             VALUES ('environment_fixture', 'yard_fixture', 'production', 'production', 'active', 1, 1);
             INSERT INTO yard_access_policies (yard_id, visibility, updated_at_ms, updated_by_principal)
             VALUES ('yard_fixture', 'any-authenticated', 1, 'fixture');",
        )
        .expect("admission fixture");
}
