#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Yard session admission drift and late validation coverage.

use blobyard_contract::{
    NewAuditEvent, NewYardContinuation, NewYardSession, RepositoryError, WebYardRepository,
    YARD_EXCHANGE_CODE_LIFETIME_MS, YARD_SESSION_LIFETIME_MS, YardSessionAuditContext,
    YardSessionRepository,
};
use blobyard_repository_sqlite::SqliteRepository;
use rusqlite::{Connection, params};

fn hash(value: char) -> String {
    value.to_string().repeat(64)
}

fn continuation(id: &str, marker: char, at: u64) -> NewYardContinuation {
    NewYardContinuation {
        id: id.to_owned(),
        continuation_hash: hash(marker),
        code_hash: hash(char::from_u32(u32::from(marker) + 1).expect("next marker")),
        yard_id: "yard_fixture".to_owned(),
        environment_id: "environment_fixture".to_owned(),
        host_label: "docs-fixture".to_owned(),
        user_id: "user_fixture".to_owned(),
        return_path: "/".to_owned(),
        created_at_ms: at,
        expires_at_ms: at + YARD_EXCHANGE_CODE_LIFETIME_MS,
    }
}

fn session(id: &str, marker: char, at: u64) -> NewYardSession {
    NewYardSession {
        id: id.to_owned(),
        token_hash: hash(marker),
        created_at_ms: at,
        expires_at_ms: at + YARD_SESSION_LIFETIME_MS,
    }
}

#[test]
fn live_admission_mismatch_and_late_failures_are_closed_atomically() {
    let temporary = tempfile::tempdir().expect("temporary");
    let path = temporary.path().join("metadata.sqlite3");
    let repository = SqliteRepository::open(&path).expect("repository");
    seed_admission(&path);
    assert_issue_failures(&repository);
    let mut tracker = blobyard_testkit::FixtureExecutionTracker::new("sqlite", "admission-drift");
    assert_live_admission_drift(&repository, &path, &mut tracker);
    tracker.finish().expect("complete admission drift fixtures");
    assert_late_failures(&repository, &path);
}

fn assert_issue_failures(repository: &SqliteRepository) {
    let mut unknown_host = continuation("continuation_unknown", 'a', 9);
    "unknown-fixture".clone_into(&mut unknown_host.host_label);
    assert_eq!(
        repository.issue_yard_exchange_code(&unknown_host),
        Err(RepositoryError::NotFound)
    );
    let mut wrong_yard = continuation("continuation_wrong", 'a', 10);
    "yard_other".clone_into(&mut wrong_yard.yard_id);
    assert_eq!(
        repository.issue_yard_exchange_code(&wrong_yard),
        Err(RepositoryError::NotFound)
    );

    let near_limit = (i64::MAX as u64) - 100;
    assert_eq!(
        repository.issue_yard_exchange_code(&continuation("continuation_time", 'c', near_limit)),
        Err(RepositoryError::InvalidInput)
    );
}

fn assert_live_admission_drift(
    repository: &SqliteRepository,
    path: &std::path::Path,
    tracker: &mut blobyard_testkit::FixtureExecutionTracker,
) {
    let durable = continuation("continuation_drift", 'e', 20);
    repository
        .issue_yard_exchange_code(&durable)
        .expect("continuation");
    replace_production_environment(path, true);
    assert_eq!(
        repository.exchange_yard_session_code(
            &durable.code_hash,
            &durable.host_label,
            &session("session_drift", '7', 21),
            &audit("audit_drift", "request_drift"),
            21,
        ),
        Err(RepositoryError::NotFound)
    );
    tracker.record_case(
        "environment-replacement-between-issue-and-exchange-denies",
        &serde_json::json!({
            "principalKind": "group",
            "change": "environment-replaced",
            "driftPoint": "after-code-issue"
        }),
        &serde_json::json!({"admitted": false, "repositoryError": "NOT_FOUND"}),
    );
    disable_replacement_environment(path);
    assert_eq!(
        repository.exchange_yard_session_code(
            &durable.code_hash,
            &durable.host_label,
            &session("session_unavailable", '8', 22),
            &audit("audit_unavailable", "request_unavailable"),
            22,
        ),
        Err(RepositoryError::NotFound)
    );
    replace_production_environment(path, false);
}

fn assert_late_failures(repository: &SqliteRepository, path: &std::path::Path) {
    let near_limit = (i64::MAX as u64) - 100;
    insert_near_expiry_continuation(path, near_limit);
    assert_eq!(
        repository.exchange_yard_session_code(
            &hash('9'),
            "docs-fixture",
            &session("session_time", 'a', near_limit),
            &audit("audit_time", "request_time"),
            near_limit,
        ),
        Err(RepositoryError::InvalidInput)
    );

    let durable = continuation("continuation_valid", '1', 30);
    repository
        .issue_yard_exchange_code(&durable)
        .expect("continuation");
    let exchange = repository
        .exchange_yard_session_code(
            &durable.code_hash,
            &durable.host_label,
            &session("session_valid", '3', 31),
            &audit("audit_valid", "request_valid"),
            31,
        )
        .expect("session");
    let duplicate_audit = continuation("continuation_audit", '5', 32);
    repository
        .issue_yard_exchange_code(&duplicate_audit)
        .expect("continuation");
    assert_eq!(
        repository.exchange_yard_session_code(
            &duplicate_audit.code_hash,
            &duplicate_audit.host_label,
            &session("session_duplicate_audit", '7', 33),
            &audit("audit_valid", "request_duplicate_audit"),
            33,
        ),
        Err(RepositoryError::Conflict)
    );
    let event = revoke_event("yard_fixture", &exchange.session.id, 34);
    assert_eq!(
        repository.revoke_yard_session("yard_other", &exchange.session.id, 34, &event),
        Err(RepositoryError::NotFound)
    );
    let mut invalid_event = event;
    "wrong.action".clone_into(&mut invalid_event.action);
    assert_eq!(
        repository.revoke_yard_session("yard_fixture", &exchange.session.id, 34, &invalid_event,),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.yard_file_by_host("docs-fixture", "", Some("invalid"), 34),
        Err(RepositoryError::InvalidInput)
    );
}

fn audit(id: &str, request_id: &str) -> YardSessionAuditContext {
    YardSessionAuditContext {
        id: id.to_owned(),
        request_id: request_id.to_owned(),
    }
}

fn revoke_event(yard_id: &str, session_id: &str, at: u64) -> NewAuditEvent {
    NewAuditEvent {
        id: "audit_revoke".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "user_fixture".to_owned(),
        action: "yard.session_revoked".to_owned(),
        request_id: "request_revoke".to_owned(),
        target_type: "yard_session".to_owned(),
        metadata: vec![
            (
                "sessionId".to_owned(),
                blobyard_contract::AuditValue::String(session_id.to_owned()),
            ),
            (
                "yardId".to_owned(),
                blobyard_contract::AuditValue::String(yard_id.to_owned()),
            ),
        ],
        created_at_ms: at,
    }
}

fn seed_admission(path: &std::path::Path) {
    Connection::open(path)
        .expect("fixture connection")
        .execute_batch(
            "INSERT INTO workspaces (id, name, slug) VALUES ('workspace_fixture', 'Fixture', 'fixture');
             INSERT INTO projects (id, workspace_id, name, slug) VALUES ('project_fixture', 'workspace_fixture', 'Fixture', 'fixture');
             INSERT INTO local_users (id, workspace_id, display_name, status, created_at_ms) VALUES ('user_fixture', 'workspace_fixture', 'Reader', 'active', 1);
             INSERT INTO yard_subjects (id, kind, workspace_id, local_user_id, created_at_ms)
             VALUES ('user_fixture', 'member', 'workspace_fixture', 'user_fixture', 1);
             INSERT INTO web_yards (id, workspace_id, project_id, name, host_label, status, created_at_ms, updated_at_ms) VALUES ('yard_fixture', 'workspace_fixture', 'project_fixture', 'docs', 'docs-fixture', 'active', 1, 1);
             INSERT INTO yard_deploys (id, yard_id, workspace_id, project_id, client_deploy_id, manifest_root, deployment_host_label, spa, clean_urls, status, created_at_ms, finalised_at_ms, file_count, total_bytes) VALUES ('deploy_fixture', 'yard_fixture', 'workspace_fixture', 'project_fixture', 'client_fixture', '.blobyard-yard/yard_fixture/client_fixture/', 'docs-deploy-fixture', 0, 0, 'live', 1, 2, 1, 0);
             UPDATE web_yards SET current_deploy_id = 'deploy_fixture' WHERE id = 'yard_fixture';
             INSERT INTO yard_environments (id, yard_id, name, kind, status, created_at_ms, updated_at_ms) VALUES ('environment_fixture', 'yard_fixture', 'production', 'production', 'active', 1, 1);
             INSERT INTO yard_access_policies (yard_id, visibility, updated_at_ms, updated_by_principal) VALUES ('yard_fixture', 'any-authenticated', 1, 'fixture');",
        )
        .expect("admission fixture");
}

fn replace_production_environment(path: &std::path::Path, drifted: bool) {
    let connection = Connection::open(path).expect("fixture connection");
    if drifted {
        connection
            .execute_batch(
                "UPDATE yard_environments SET status = 'deleted', deleted_at_ms = 2 WHERE id = 'environment_fixture';
                 INSERT INTO yard_environments (id, yard_id, name, kind, status, created_at_ms, updated_at_ms) VALUES ('environment_other', 'yard_fixture', 'replacement', 'production', 'active', 2, 2);",
            )
            .expect("drift environment");
    } else {
        connection
            .execute_batch(
                "DELETE FROM yard_environments WHERE id = 'environment_other';
                 UPDATE yard_environments SET status = 'active', deleted_at_ms = NULL WHERE id = 'environment_fixture';",
            )
            .expect("restore environment");
    }
}

fn disable_replacement_environment(path: &std::path::Path) {
    Connection::open(path)
        .expect("fixture connection")
        .execute(
            "UPDATE yard_environments SET status = 'deleted', deleted_at_ms = 3 WHERE id = 'environment_other'",
            [],
        )
        .expect("disable replacement environment");
}

fn insert_near_expiry_continuation(path: &std::path::Path, at: u64) {
    Connection::open(path)
        .expect("fixture connection")
        .execute(
            "INSERT INTO yard_continuations
             (id, continuation_hash, code_hash, yard_id, environment_id, host_label, subject_id,
              return_path, created_at_ms, expires_at_ms)
             VALUES
             ('continuation_near', ?1, ?2, 'yard_fixture', 'environment_fixture',
              'docs-fixture', 'user_fixture', '/', ?3, ?4)",
            params![
                hash('8'),
                hash('9'),
                i64::try_from(at - 1).expect("created"),
                i64::MAX,
            ],
        )
        .expect("near expiry continuation");
}
