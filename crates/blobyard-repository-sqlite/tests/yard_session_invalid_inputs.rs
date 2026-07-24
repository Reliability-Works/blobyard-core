#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Exhaustive Yard session input-validation coverage.

use blobyard_contract::{
    NewAuditEvent, NewYardContinuation, NewYardSession, RepositoryError, WebYardRepository,
    YARD_EXCHANGE_CODE_LIFETIME_MS, YARD_SESSION_LIFETIME_MS, YardSessionAuditContext,
    YardSessionRepository,
};
use blobyard_repository_sqlite::SqliteRepository;

fn hash(value: char) -> String {
    value.to_string().repeat(64)
}

fn continuation(return_path: &str) -> NewYardContinuation {
    NewYardContinuation {
        id: "continuation_fixture".to_owned(),
        continuation_hash: hash('a'),
        code_hash: hash('b'),
        yard_id: "yard_fixture".to_owned(),
        environment_id: "environment_fixture".to_owned(),
        host_label: "docs-fixture".to_owned(),
        user_id: "user_fixture".to_owned(),
        return_path: return_path.to_owned(),
        created_at_ms: 10,
        expires_at_ms: 10 + YARD_EXCHANGE_CODE_LIFETIME_MS,
    }
}

fn repository() -> (tempfile::TempDir, SqliteRepository) {
    let temporary = tempfile::tempdir().expect("temporary");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");
    (temporary, repository)
}

#[test]
fn continuation_and_admission_validate_every_boundary_before_lookup() {
    let (_temporary, repository) = repository();
    for mutate in [
        |value: &mut NewYardContinuation| value.id.clear(),
        |value: &mut NewYardContinuation| value.yard_id.clear(),
        |value: &mut NewYardContinuation| value.environment_id.clear(),
        |value: &mut NewYardContinuation| value.user_id.clear(),
        |value: &mut NewYardContinuation| value.code_hash = "invalid".to_owned(),
        |value: &mut NewYardContinuation| value.host_label = "invalid".to_owned(),
        |value: &mut NewYardContinuation| value.created_at_ms = u64::MAX,
    ] {
        let mut malformed = continuation("/");
        mutate(&mut malformed);
        assert_eq!(
            repository.issue_yard_exchange_code(&malformed),
            Err(RepositoryError::InvalidInput)
        );
    }
    let overflow_time = (i64::MAX as u64) + 1;
    let overflow_continuation = NewYardContinuation {
        created_at_ms: overflow_time,
        expires_at_ms: overflow_time + YARD_EXCHANGE_CODE_LIFETIME_MS,
        ..continuation("/")
    };
    assert_eq!(
        repository.issue_yard_exchange_code(&overflow_continuation),
        Err(RepositoryError::InvalidInput)
    );
    let long_path = format!("/{}", "x".repeat(2_049));
    for return_path in [
        "//external",
        "/\\ambiguous",
        "/bad\npath",
        "/.blobyard",
        "/.blobyard/private",
        &long_path,
    ] {
        assert_eq!(
            repository.issue_yard_exchange_code(&continuation(return_path)),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        repository.evaluate_yard_admission("docs-fixture", "", 10),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.evaluate_yard_admission("", "user_fixture", 10),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.evaluate_yard_admission("docs-fixture", "user_fixture", u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn session_exchange_validates_every_boundary_before_lookup() {
    let (_temporary, repository) = repository();
    let valid_session = valid_session();
    let valid_audit = valid_audit();
    assert_session_fields(&repository, &valid_session, &valid_audit);
    assert_exchange_fields(&repository, &valid_session, &valid_audit);
}

fn assert_session_fields(
    repository: &SqliteRepository,
    valid_session: &NewYardSession,
    valid_audit: &YardSessionAuditContext,
) {
    for (session, audit) in [
        (
            NewYardSession {
                id: String::new(),
                ..valid_session.clone()
            },
            valid_audit.clone(),
        ),
        (
            NewYardSession {
                token_hash: "invalid".to_owned(),
                ..valid_session.clone()
            },
            valid_audit.clone(),
        ),
        (
            valid_session.clone(),
            YardSessionAuditContext {
                id: String::new(),
                ..valid_audit.clone()
            },
        ),
        (
            valid_session.clone(),
            YardSessionAuditContext {
                request_id: String::new(),
                ..valid_audit.clone()
            },
        ),
        (
            NewYardSession {
                created_at_ms: 19,
                ..valid_session.clone()
            },
            valid_audit.clone(),
        ),
    ] {
        assert_eq!(
            repository
                .exchange_yard_session_code(&hash('c'), "docs-fixture", &session, &audit, 20,),
            Err(RepositoryError::InvalidInput)
        );
    }
}

fn assert_exchange_fields(
    repository: &SqliteRepository,
    valid_session: &NewYardSession,
    valid_audit: &YardSessionAuditContext,
) {
    for (code_hash, host_label) in [("invalid", "docs-fixture"), (&hash('c'), "invalid")] {
        assert_eq!(
            repository.exchange_yard_session_code(
                code_hash,
                host_label,
                valid_session,
                valid_audit,
                20,
            ),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        repository.exchange_yard_session_code(&hash('c'), "", valid_session, valid_audit, 20,),
        Err(RepositoryError::InvalidInput)
    );
    let overflow_time = (i64::MAX as u64) + 1;
    assert_eq!(
        repository.exchange_yard_session_code(
            &hash('c'),
            "docs-fixture",
            &NewYardSession {
                created_at_ms: overflow_time,
                expires_at_ms: overflow_time + YARD_SESSION_LIFETIME_MS,
                ..valid_session.clone()
            },
            valid_audit,
            overflow_time,
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn revocation_validates_every_boundary_before_lookup() {
    let (_temporary, repository) = repository();
    let event = revoked_event();
    for (yard_id, session_id) in [("", "session_fixture"), ("yard_fixture", "")] {
        assert_eq!(
            repository.revoke_yard_session(yard_id, session_id, 20, &event),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        repository.revoke_yard_session_by_token("invalid", "docs-fixture", 20),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.revoke_yard_session_by_token(&hash('e'), "", 20),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.revoke_yard_session_by_token(&hash('e'), "docs-fixture", u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.revoke_yard_session("yard_fixture", "session_missing", 20, &event),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.yard_file_by_host("docs-fixture", "", None, u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
}

fn valid_session() -> NewYardSession {
    NewYardSession {
        id: "session_fixture".to_owned(),
        token_hash: hash('d'),
        created_at_ms: 20,
        expires_at_ms: 20 + YARD_SESSION_LIFETIME_MS,
    }
}

fn valid_audit() -> YardSessionAuditContext {
    YardSessionAuditContext {
        id: "audit_fixture".to_owned(),
        request_id: "request_fixture".to_owned(),
    }
}

fn revoked_event() -> NewAuditEvent {
    NewAuditEvent {
        id: "audit_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "actor_fixture".to_owned(),
        action: "yard.session_revoked".to_owned(),
        request_id: "request_fixture".to_owned(),
        target_type: "yard_session".to_owned(),
        metadata: Vec::new(),
        created_at_ms: 20,
    }
}
