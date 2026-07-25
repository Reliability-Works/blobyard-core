#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Fail-closed admission coverage for lifecycle and membership corruption.

use blobyard_contract::{
    LifecycleRepository, NewYardContinuation, NewYardSession, RepositoryError, WebYardRepository,
    YARD_EXCHANGE_CODE_LIFETIME_MS, YARD_SESSION_LIFETIME_MS, YardSessionAuditContext,
    YardSessionRepository,
};
use blobyard_repository_sqlite::SqliteRepository;
use rusqlite::Connection;

const FIXTURE_SQL: &str = include_str!("support/yard_group_race.sql");

#[derive(Clone, Copy, Debug)]
enum Corruption {
    ActiveGrantWithRevocation,
    ActiveGroupWithDeactivation,
    InvalidMembershipTimestamp,
    IncorrectMemberCount,
    NonmatchingEnvironment,
}

const CORRUPTIONS: [Corruption; 5] = [
    Corruption::ActiveGrantWithRevocation,
    Corruption::ActiveGroupWithDeactivation,
    Corruption::InvalidMembershipTimestamp,
    Corruption::IncorrectMemberCount,
    Corruption::NonmatchingEnvironment,
];

struct Fixture {
    _temporary: tempfile::TempDir,
    path: std::path::PathBuf,
    repository: SqliteRepository,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary");
        let path = temporary.path().join("metadata.sqlite3");
        let repository = SqliteRepository::open(&path).expect("repository");
        Connection::open(&path)
            .expect("fixture connection")
            .execute_batch(FIXTURE_SQL)
            .expect("fixture");
        Self {
            _temporary: temporary,
            path,
            repository,
        }
    }

    fn set_corruption(&self, corruption: Corruption, corrupt: bool) {
        let sql = match (corruption, corrupt) {
            (Corruption::ActiveGrantWithRevocation, true) => {
                "PRAGMA ignore_check_constraints = ON;
                 UPDATE yard_access_grants SET revoked_at_ms = 3 WHERE id = 'grant_fixture';"
            }
            (Corruption::ActiveGrantWithRevocation, false) => {
                "UPDATE yard_access_grants SET revoked_at_ms = NULL WHERE id = 'grant_fixture';"
            }
            (Corruption::ActiveGroupWithDeactivation, true) => {
                "PRAGMA ignore_check_constraints = ON;
                 UPDATE workspace_groups SET deactivated_at_ms = 3
                 WHERE id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
            }
            (Corruption::ActiveGroupWithDeactivation, false) => {
                "UPDATE workspace_groups SET deactivated_at_ms = NULL
                 WHERE id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
            }
            (Corruption::InvalidMembershipTimestamp, true) => {
                "PRAGMA ignore_check_constraints = ON;
                 UPDATE workspace_group_members SET added_at_ms = -1
                 WHERE group_id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
            }
            (Corruption::InvalidMembershipTimestamp, false) => {
                "UPDATE workspace_group_members SET added_at_ms = 2
                 WHERE group_id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
            }
            (Corruption::IncorrectMemberCount, true) => {
                "UPDATE workspace_groups SET member_count = 2
                 WHERE id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
            }
            (Corruption::IncorrectMemberCount, false) => {
                "UPDATE workspace_groups SET member_count = 1
                 WHERE id = 'group_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';"
            }
            (Corruption::NonmatchingEnvironment, true) => {
                "INSERT INTO yard_environments
                   (id, yard_id, name, kind, status, created_at_ms, updated_at_ms)
                 VALUES
                   ('environment_staging', 'yard_fixture', 'staging', 'staging', 'active', 2, 2);
                 UPDATE yard_access_grants SET environment_id = 'environment_staging'
                 WHERE id = 'grant_fixture';"
            }
            (Corruption::NonmatchingEnvironment, false) => {
                "UPDATE yard_access_grants SET environment_id = NULL WHERE id = 'grant_fixture';
                 DELETE FROM yard_environments WHERE id = 'environment_staging';"
            }
        };
        Connection::open(&self.path)
            .expect("corruption connection")
            .execute_batch(sql)
            .expect("corruption state");
    }
}

#[test]
fn corrupt_group_lifecycle_is_concealed_at_every_admission_boundary() {
    for corruption in CORRUPTIONS {
        assert_issue_conceals(corruption);
        assert_exchange_conceals_and_rolls_back(corruption);
        assert_delivery_conceals(corruption);
    }
}

fn assert_issue_conceals(corruption: Corruption) {
    let fixture = Fixture::new();
    let continuation = continuation();
    fixture.set_corruption(corruption, true);
    assert_eq!(
        fixture.repository.issue_yard_exchange_code(&continuation),
        Err(RepositoryError::NotFound),
        "{corruption:?}"
    );
    fixture.set_corruption(corruption, false);
    fixture
        .repository
        .issue_yard_exchange_code(&continuation)
        .expect("failed issue must not persist a continuation");
}

fn assert_exchange_conceals_and_rolls_back(corruption: Corruption) {
    let fixture = Fixture::new();
    let continuation = continuation();
    fixture
        .repository
        .issue_yard_exchange_code(&continuation)
        .expect("issue");
    let session = session();
    let audit = audit();
    fixture.set_corruption(corruption, true);
    assert_eq!(
        fixture.repository.exchange_yard_session_code(
            &continuation.code_hash,
            &continuation.host_label,
            &session,
            &audit,
            11,
        ),
        Err(RepositoryError::NotFound),
        "{corruption:?}"
    );
    fixture.set_corruption(corruption, false);
    fixture
        .repository
        .exchange_yard_session_code(
            &continuation.code_hash,
            &continuation.host_label,
            &session,
            &audit,
            11,
        )
        .expect("failed exchange must not consume the code");
    assert_eq!(
        fixture
            .repository
            .list_audit("workspace_fixture", None, 50)
            .expect("audits")
            .items
            .iter()
            .filter(|event| event.id == audit.id)
            .count(),
        1
    );
}

fn assert_delivery_conceals(corruption: Corruption) {
    let fixture = Fixture::new();
    let continuation = continuation();
    fixture
        .repository
        .issue_yard_exchange_code(&continuation)
        .expect("issue");
    let session = session();
    fixture
        .repository
        .exchange_yard_session_code(
            &continuation.code_hash,
            &continuation.host_label,
            &session,
            &audit(),
            11,
        )
        .expect("exchange");
    fixture.set_corruption(corruption, true);
    assert_eq!(
        fixture.repository.yard_file_by_host(
            &continuation.host_label,
            "asset.js",
            Some(&session.token_hash),
            12,
        ),
        Err(RepositoryError::NotFound),
        "{corruption:?}"
    );
    fixture.set_corruption(corruption, false);
    fixture
        .repository
        .yard_file_by_host(
            &continuation.host_label,
            "asset.js",
            Some(&session.token_hash),
            12,
        )
        .expect("restored delivery");
}

fn continuation() -> NewYardContinuation {
    NewYardContinuation {
        id: "continuation_corruption".to_owned(),
        continuation_hash: hash('c'),
        code_hash: hash('e'),
        yard_id: "yard_fixture".to_owned(),
        environment_id: "environment_fixture".to_owned(),
        host_label: "docs-fixture".to_owned(),
        user_id: "user_fixture".to_owned(),
        return_path: "/".to_owned(),
        created_at_ms: 10,
        expires_at_ms: 10 + YARD_EXCHANGE_CODE_LIFETIME_MS,
    }
}

fn session() -> NewYardSession {
    NewYardSession {
        id: "session_corruption".to_owned(),
        token_hash: hash('f'),
        created_at_ms: 11,
        expires_at_ms: 11 + YARD_SESSION_LIFETIME_MS,
    }
}

fn audit() -> YardSessionAuditContext {
    YardSessionAuditContext {
        id: "audit_corruption".to_owned(),
        request_id: "request_corruption".to_owned(),
    }
}

fn hash(value: char) -> String {
    value.to_string().repeat(64)
}
