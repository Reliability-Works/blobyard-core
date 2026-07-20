#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use blobyard_contract::InboxRepository;

#[path = "inboxes_tests/uploads.rs"]
mod uploads;

#[path = "inboxes_tests/corruption.rs"]
mod corruption;

#[path = "inboxes_tests/transactions.rs"]
mod transactions;

struct Fixture {
    _temporary: tempfile::TempDir,
    repository: SqliteRepository,
    inbox: NewInbox,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let repository =
            SqliteRepository::open(&temporary.path().join("inboxes.sqlite3")).expect("repository");
        blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
        Self {
            _temporary: temporary,
            repository,
            inbox: NewInbox {
                id: "inbox_validation".to_owned(),
                workspace_id: "workspace_fixture".to_owned(),
                project_id: "project_fixture".to_owned(),
                name: "Validation inbox".to_owned(),
                capability_hash: hash('a'),
                expires_at_ms: 5_000,
                maximum_files: 2,
                maximum_bytes: 10,
                created_at_ms: 1_000,
            },
        }
    }

    fn create(&self) {
        self.repository
            .create_inbox(&self.inbox, &event("inbox.created", &self.inbox, 1_000))
            .expect("inbox");
    }
}

fn hash(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}

fn event(action: &str, inbox: &NewInbox, created_at_ms: u64) -> NewAuditEvent {
    blobyard_testkit::inbox_event(action, &inbox.id, created_at_ms)
}

fn upload(id: &str, created_at_ms: u64) -> NewUploadReservation {
    NewUploadReservation {
        id: id.to_owned(),
        project_id: "project_fixture".to_owned(),
        object_path: format!("inbox/{id}.bin"),
        filename: format!("{id}.bin"),
        content_type: "application/octet-stream".to_owned(),
        expected_size: 5,
        expected_checksum: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
            .to_owned(),
        storage_key: format!("objects/{id}"),
        capability_hash: format!("{created_at_ms:064x}"),
        expires_at_ms: 4_000,
        created_at_ms,
        source: blobyard_contract::ObjectSource::Inbox,
        git_repository: None,
        git_commit: None,
        git_branch: None,
        strategy: blobyard_contract::ReservationStrategy::Single,
        part_size: None,
        part_count: None,
    }
}

fn principal(inbox: &NewInbox, now_ms: u64) -> blobyard_contract::NewInboxUpload {
    blobyard_contract::NewInboxUpload {
        capability_hash: inbox.capability_hash.clone(),
        fingerprint_hash: hash('d'),
        now_ms,
    }
}

#[test]
fn creation_rejects_invalid_fields_targets_and_audit() {
    let fixture = Fixture::new();
    let mut foreign = fixture.inbox.clone();
    foreign.workspace_id = "workspace_foreign".to_owned();
    let mut foreign_event = event("inbox.created", &foreign, foreign.created_at_ms);
    foreign_event.workspace_id.clone_from(&foreign.workspace_id);
    assert_eq!(
        fixture.repository.create_inbox(&foreign, &foreign_event),
        Err(RepositoryError::NotFound)
    );
    let mutations: [fn(&mut NewInbox); 13] = [
        |inbox| inbox.id.clear(),
        |inbox| inbox.workspace_id.clear(),
        |inbox| inbox.project_id.clear(),
        |inbox| inbox.name.clear(),
        |inbox| inbox.name = "x".repeat(129),
        |inbox| inbox.name = "bad\nname".to_owned(),
        |inbox| inbox.capability_hash = "invalid".to_owned(),
        |inbox| inbox.expires_at_ms = inbox.created_at_ms,
        |inbox| inbox.maximum_files = 0,
        |inbox| inbox.maximum_bytes = 0,
        |inbox| inbox.expires_at_ms = u64::MAX,
        |inbox| inbox.maximum_files = u64::MAX,
        |inbox| inbox.maximum_bytes = u64::MAX,
    ];
    for mutate in mutations {
        let mut invalid = fixture.inbox.clone();
        mutate(&mut invalid);
        assert_eq!(
            fixture.repository.create_inbox(
                &invalid,
                &event("inbox.created", &invalid, invalid.created_at_ms),
            ),
            Err(RepositoryError::InvalidInput)
        );
    }
    let mut invalid_time = fixture.inbox.clone();
    invalid_time.created_at_ms = u64::MAX;
    assert_eq!(
        fixture.repository.create_inbox(
            &invalid_time,
            &event("inbox.created", &invalid_time, u64::MAX),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .repository
            .create_inbox(&fixture.inbox, &event("inbox.wrong", &fixture.inbox, 1_000),),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn queries_and_rate_limits_reject_invalid_boundaries() {
    let fixture = Fixture::new();
    fixture.create();
    assert_eq!(
        fixture.repository.list_inboxes(""),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.inbox_by_capability("invalid", 1_001),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .repository
            .inbox_by_capability(&fixture.inbox.capability_hash, u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
    for (key, window, limit, now) in [
        ("invalid", 1_000, 1, 1_000),
        (hash('b').as_str(), 0, 1, 1_000),
        (hash('b').as_str(), 1_000, 0, 1_000),
        (hash('b').as_str(), 1_000, 1, u64::MAX),
        (hash('b').as_str(), u64::MAX, 1, 1_000),
    ] {
        assert_eq!(
            fixture
                .repository
                .consume_inbox_rate(key, window, limit, now),
            Err(RepositoryError::InvalidInput)
        );
    }
}

#[test]
fn newest_first_listing_and_revocation_are_exact() {
    let fixture = Fixture::new();
    fixture.create();
    let second = NewInbox {
        id: "inbox_second".to_owned(),
        capability_hash: hash('c'),
        created_at_ms: 1_001,
        ..fixture.inbox.clone()
    };
    fixture
        .repository
        .create_inbox(&second, &event("inbox.created", &second, 1_001))
        .expect("second inbox");
    let listed = fixture
        .repository
        .list_inboxes(&fixture.inbox.project_id)
        .expect("listed inboxes");
    assert_eq!(listed[0].id, second.id);
    assert_eq!(listed[1].id, fixture.inbox.id);
    assert_eq!(
        fixture.repository.revoke_inbox(
            &fixture.inbox.id,
            "workspace_foreign",
            1_100,
            &event("inbox.revoked", &fixture.inbox, 1_100),
        ),
        Err(RepositoryError::NotFound)
    );
    for (timestamp, action) in [(999, "inbox.revoked"), (1_100, "inbox.wrong")] {
        assert_eq!(
            fixture.repository.revoke_inbox(
                &fixture.inbox.id,
                &fixture.inbox.workspace_id,
                timestamp,
                &event(action, &fixture.inbox, timestamp),
            ),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        fixture.repository.revoke_inbox(
            &fixture.inbox.id,
            &fixture.inbox.workspace_id,
            u64::MAX,
            &event("inbox.revoked", &fixture.inbox, u64::MAX),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn revocation_rejects_invalid_identifiers_and_suppressed_updates() {
    let fixture = Fixture::new();
    fixture.create();
    for (inbox_id, workspace_id) in [
        ("", fixture.inbox.workspace_id.as_str()),
        (fixture.inbox.id.as_str(), ""),
    ] {
        assert_eq!(
            fixture.repository.revoke_inbox(
                inbox_id,
                workspace_id,
                1_100,
                &event("inbox.revoked", &fixture.inbox, 1_100),
            ),
            Err(RepositoryError::InvalidInput)
        );
    }
    fixture
        .repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER suppress_inbox_revoke BEFORE UPDATE OF status ON inboxes
             WHEN NEW.status = 'revoked' BEGIN SELECT RAISE(IGNORE); END;",
        )
        .expect("suppressing trigger");
    assert_eq!(
        fixture.repository.revoke_inbox(
            &fixture.inbox.id,
            &fixture.inbox.workspace_id,
            1_100,
            &event("inbox.revoked", &fixture.inbox, 1_100),
        ),
        Err(RepositoryError::Conflict)
    );
}
