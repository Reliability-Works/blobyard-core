#![allow(clippy::expect_used, reason = "test synchronization must fail loudly")]
//! Group-admission races through the public `SQLite` adapter.

use blobyard_contract::{
    AuditValue, NewYardContinuation, NewYardSession, RepositoryError, WebYardRepository,
    WorkspaceGroupRepository, WorkspaceGroupStatus, YARD_EXCHANGE_CODE_LIFETIME_MS,
    YARD_SESSION_LIFETIME_MS, YardSessionAuditContext, YardSessionRepository,
};
use blobyard_repository_sqlite::SqliteRepository;
use std::sync::{Arc, Barrier};

#[test]
fn concurrent_issue_of_one_continuation_has_one_winner() {
    let fixture = Fixture::new();
    let continuation = continuation("continuation_issue_race", '1', 10);
    let barrier = Arc::new(Barrier::new(2));
    let results = std::thread::scope(|scope| {
        let first = spawn_issue(scope, &fixture.repository, &barrier, &continuation);
        let second = spawn_issue(scope, &fixture.repository, &barrier, &continuation);
        [first.join().expect("first"), second.join().expect("second")]
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| **result == Err(RepositoryError::Conflict))
            .count(),
        1
    );
}

#[test]
fn group_grant_revocation_racing_exchange_fails_closed() {
    let fixture = Fixture::new();
    let continuation = continuation("continuation_exchange_race", '2', 20);
    fixture
        .repository
        .issue_yard_exchange_code(&continuation)
        .expect("issue");
    let barrier = Arc::new(Barrier::new(2));
    let exchange = std::thread::scope(|scope| {
        let exchange = spawn_exchange(scope, &fixture.repository, &barrier, &continuation);
        let revoke = spawn_revoke(scope, &fixture.repository, &barrier);
        let exchange = exchange.join().expect("exchange");
        assert!(revoke.join().expect("revoke").expect("revoke result"));
        exchange
    });
    assert!(matches!(exchange, Ok(_) | Err(RepositoryError::NotFound)));
    if let Ok(exchange) = exchange {
        assert_eq!(
            fixture.repository.yard_file_by_host(
                "docs-fixture",
                "asset.js",
                Some(&exchange.session.token_hash),
                22,
            ),
            Err(RepositoryError::NotFound)
        );
    }
}

#[test]
fn membership_removal_racing_delivery_denies_the_next_request() {
    let fixture = Fixture::new();
    let session = issue_session(&fixture.repository);
    let barrier = Arc::new(Barrier::new(2));
    let delivery = std::thread::scope(|scope| {
        let delivery = spawn_delivery(scope, &fixture.repository, &barrier, &session.token_hash);
        let removal = spawn_removal(scope, &fixture.repository, &barrier);
        let delivery = delivery.join().expect("delivery");
        removal.join().expect("removal").expect("remove member");
        delivery
    });
    assert!(matches!(delivery, Ok(_) | Err(RepositoryError::NotFound)));
    assert_eq!(
        fixture.repository.yard_file_by_host(
            "docs-fixture",
            "asset.js",
            Some(&session.token_hash),
            33,
        ),
        Err(RepositoryError::NotFound)
    );
}

struct Fixture {
    _temporary: tempfile::TempDir,
    repository: SqliteRepository,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary");
        let repository =
            SqliteRepository::open(&temporary.path().join("races.sqlite3")).expect("repository");
        repository
            .test_connection()
            .expect("connection")
            .execute_batch(include_str!("support/yard_group_race.sql"))
            .expect("race fixture");
        Self {
            _temporary: temporary,
            repository,
        }
    }
}

fn spawn_issue<'scope, 'environment>(
    scope: &'scope std::thread::Scope<'scope, 'environment>,
    repository: &'environment SqliteRepository,
    barrier: &'environment Barrier,
    continuation: &'environment NewYardContinuation,
) -> std::thread::ScopedJoinHandle<'scope, Result<(), RepositoryError>> {
    scope.spawn(move || {
        barrier.wait();
        repository.issue_yard_exchange_code(continuation)
    })
}

fn spawn_exchange<'scope, 'environment>(
    scope: &'scope std::thread::Scope<'scope, 'environment>,
    repository: &'environment SqliteRepository,
    barrier: &'environment Barrier,
    continuation: &'environment NewYardContinuation,
) -> std::thread::ScopedJoinHandle<
    'scope,
    Result<blobyard_contract::YardSessionExchange, RepositoryError>,
> {
    scope.spawn(move || {
        barrier.wait();
        repository.exchange_yard_session_code(
            &continuation.code_hash,
            &continuation.host_label,
            &session("session_exchange_race", '4', 21),
            &audit("audit_exchange_race"),
            21,
        )
    })
}

fn spawn_revoke<'scope, 'environment>(
    scope: &'scope std::thread::Scope<'scope, 'environment>,
    repository: &'environment SqliteRepository,
    barrier: &'environment Barrier,
) -> std::thread::ScopedJoinHandle<'scope, Result<bool, RepositoryError>> {
    scope.spawn(move || {
        barrier.wait();
        repository.revoke_yard_access_grant(
            "yard_fixture",
            "grant_fixture",
            21,
            &blobyard_testkit::revoked_event("yard_fixture", "grant_fixture", 21),
        )
    })
}

fn spawn_delivery<'scope, 'environment>(
    scope: &'scope std::thread::Scope<'scope, 'environment>,
    repository: &'environment SqliteRepository,
    barrier: &'environment Barrier,
    token_hash: &'environment str,
) -> std::thread::ScopedJoinHandle<'scope, Result<blobyard_contract::YardFileTarget, RepositoryError>>
{
    scope.spawn(move || {
        barrier.wait();
        repository.yard_file_by_host("docs-fixture", "asset.js", Some(token_hash), 32)
    })
}

fn spawn_removal<'scope, 'environment>(
    scope: &'scope std::thread::Scope<'scope, 'environment>,
    repository: &'environment SqliteRepository,
    barrier: &'environment Barrier,
) -> std::thread::ScopedJoinHandle<'scope, Result<(), RepositoryError>> {
    scope.spawn(move || {
        barrier.wait();
        let group = group();
        repository.remove_workspace_group_member(
            &group.workspace_id,
            &group.id,
            "user_fixture",
            &blobyard_testkit::group_event(
                "audit_race_member_removed",
                "group.member_removed",
                &group,
                32,
                [("userId", AuditValue::String("user_fixture".to_owned()))],
            ),
        )
    })
}

fn issue_session(repository: &SqliteRepository) -> blobyard_contract::YardSessionRecord {
    let continuation = continuation("continuation_delivery_race", '3', 30);
    repository
        .issue_yard_exchange_code(&continuation)
        .expect("issue");
    repository
        .exchange_yard_session_code(
            &continuation.code_hash,
            &continuation.host_label,
            &session("session_delivery_race", '5', 31),
            &audit("audit_delivery_race"),
            31,
        )
        .expect("exchange")
        .session
}

fn continuation(id: &str, marker: char, at_ms: u64) -> NewYardContinuation {
    NewYardContinuation {
        id: id.to_owned(),
        continuation_hash: hash(char::from_u32(u32::from(marker) + 1).expect("next marker")),
        code_hash: hash(marker),
        yard_id: "yard_fixture".to_owned(),
        environment_id: "environment_fixture".to_owned(),
        host_label: "docs-fixture".to_owned(),
        user_id: "user_fixture".to_owned(),
        return_path: "/".to_owned(),
        created_at_ms: at_ms,
        expires_at_ms: at_ms + YARD_EXCHANGE_CODE_LIFETIME_MS,
    }
}

fn session(id: &str, marker: char, at_ms: u64) -> NewYardSession {
    NewYardSession {
        id: id.to_owned(),
        token_hash: hash(marker),
        created_at_ms: at_ms,
        expires_at_ms: at_ms + YARD_SESSION_LIFETIME_MS,
    }
}

fn audit(id: &str) -> YardSessionAuditContext {
    YardSessionAuditContext {
        id: id.to_owned(),
        request_id: format!("request_{id}"),
    }
}

fn group() -> blobyard_contract::WorkspaceGroupRecord {
    blobyard_contract::WorkspaceGroupRecord {
        id: "group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        name: "Readers".to_owned(),
        status: WorkspaceGroupStatus::Active,
        member_count: 1,
        created_at_ms: 2,
        deactivated_at_ms: None,
    }
}

fn hash(marker: char) -> String {
    marker.to_string().repeat(64)
}
