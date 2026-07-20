#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use blobyard_contract::SharingRepository;

#[path = "sharing_tests/corruption.rs"]
mod corruption;

struct Fixture {
    _temporary: tempfile::TempDir,
    repository: SqliteRepository,
    share: NewShare,
}

impl Fixture {
    fn new() -> Self {
        let (temporary, repository) = super::super::repository_with_transfers();
        Self {
            _temporary: temporary,
            repository,
            share: NewShare {
                id: "share_validation".to_owned(),
                workspace_id: "workspace_fixture".to_owned(),
                version_id: "upload_two".to_owned(),
                capability_hash: "e".repeat(64),
                expires_at_ms: 5_000,
                maximum_downloads: None,
                created_at_ms: 1_000,
            },
        }
    }

    fn create(&self) {
        self.repository
            .create_share(&self.share, &event("share.created", &self.share, 1_000))
            .expect("share");
    }
}

fn event(action: &str, share: &NewShare, created_at_ms: u64) -> NewAuditEvent {
    blobyard_testkit::share_event(action, &share.id, created_at_ms)
}

fn grant(share: &NewShare, expires_at_ms: u64) -> NewDownloadGrant {
    NewDownloadGrant {
        version_id: share.version_id.clone(),
        capability_hash: "f".repeat(64),
        expires_at_ms,
    }
}

#[test]
fn share_creation_rejects_invalid_bounds_targets_and_audit() {
    let fixture = Fixture::new();
    let mut invalid = fixture.share.clone();
    invalid.version_id = "missing".to_owned();
    assert_eq!(
        fixture
            .repository
            .create_share(&invalid, &event("share.created", &invalid, 1_000)),
        Err(RepositoryError::NotFound)
    );
    for mutate in [
        |share: &mut NewShare| share.id.clear(),
        |share: &mut NewShare| share.capability_hash = "invalid".to_owned(),
        |share: &mut NewShare| share.expires_at_ms = share.created_at_ms,
        |share: &mut NewShare| share.maximum_downloads = Some(0),
        |share: &mut NewShare| share.expires_at_ms = u64::MAX,
        |share: &mut NewShare| share.maximum_downloads = Some(u64::MAX),
        |share: &mut NewShare| {
            share.created_at_ms = u64::MAX;
            share.expires_at_ms = i64::MAX as u64;
        },
    ] {
        let mut invalid = fixture.share.clone();
        mutate(&mut invalid);
        assert_eq!(
            fixture.repository.create_share(
                &invalid,
                &event("share.created", &invalid, invalid.created_at_ms),
            ),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        fixture.repository.create_share(
            &fixture.share,
            &event("share.wrong", &fixture.share, fixture.share.created_at_ms),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn share_queries_reject_invalid_inputs_and_unrepresentable_times() {
    let fixture = Fixture::new();
    fixture.create();
    assert_eq!(
        fixture.repository.list_shares(""),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.share_by_capability("invalid", 1_001),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .repository
            .share_by_capability(&fixture.share.capability_hash, u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.issue_share_download(
            "invalid",
            1_001,
            &grant(&fixture.share, 1_100),
            &event("share.download_issued", &fixture.share, 1_001),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.issue_share_download(
            &fixture.share.capability_hash,
            u64::MAX,
            &grant(&fixture.share, 1_100),
            &event("share.download_issued", &fixture.share, u64::MAX),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn share_download_rejects_invalid_grants_and_events() {
    let fixture = Fixture::new();
    fixture.create();
    let valid_event = event("share.download_issued", &fixture.share, 1_001);
    for invalid in [
        NewDownloadGrant {
            version_id: "wrong".to_owned(),
            ..grant(&fixture.share, 1_100)
        },
        NewDownloadGrant {
            version_id: String::new(),
            ..grant(&fixture.share, 1_100)
        },
        grant(&fixture.share, 6_000),
        grant(&fixture.share, 1_001),
        NewDownloadGrant {
            capability_hash: "invalid".to_owned(),
            ..grant(&fixture.share, 1_100)
        },
    ] {
        assert_eq!(
            fixture.repository.issue_share_download(
                &fixture.share.capability_hash,
                1_001,
                &invalid,
                &valid_event,
            ),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        fixture.repository.issue_share_download(
            &fixture.share.capability_hash,
            1_001,
            &grant(&fixture.share, 1_100),
            &event("share.wrong", &fixture.share, 1_001),
        ),
        Err(RepositoryError::InvalidInput)
    );
    let unrepresentable = NewDownloadGrant {
        expires_at_ms: u64::MAX,
        ..grant(&fixture.share, 1_100)
    };
    assert_eq!(
        fixture.repository.issue_share_download(
            &fixture.share.capability_hash,
            1_001,
            &unrepresentable,
            &valid_event,
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn share_download_rejects_durable_counter_overflow() {
    let fixture = Fixture::new();
    fixture.create();
    fixture
        .repository
        .test_connection()
        .expect("connection")
        .execute(
            "UPDATE shares SET consumed_count = ?1 WHERE id = ?2",
            params![i64::MAX, fixture.share.id],
        )
        .expect("maximum count");
    assert_eq!(
        fixture.repository.issue_share_download(
            &fixture.share.capability_hash,
            1_001,
            &grant(&fixture.share, 1_100),
            &event("share.download_issued", &fixture.share, 1_001),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn share_revocation_rejects_invalid_inputs_foreign_and_audit() {
    let fixture = Fixture::new();
    fixture.create();
    for (share_id, workspace_id) in [
        ("", fixture.share.workspace_id.as_str()),
        (fixture.share.id.as_str(), ""),
    ] {
        assert_eq!(
            fixture.repository.revoke_share(
                share_id,
                workspace_id,
                1_001,
                &event("share.revoked", &fixture.share, 1_001),
            ),
            Err(RepositoryError::InvalidInput)
        );
    }
    assert_eq!(
        fixture.repository.revoke_share(
            &fixture.share.id,
            "workspace_foreign",
            1_001,
            &event("share.revoked", &fixture.share, 1_001),
        ),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        fixture.repository.revoke_share(
            &fixture.share.id,
            &fixture.share.workspace_id,
            1_001,
            &event("share.wrong", &fixture.share, 1_001),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.revoke_share(
            &fixture.share.id,
            &fixture.share.workspace_id,
            u64::MAX,
            &event("share.revoked", &fixture.share, u64::MAX),
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn share_revocation_rejects_suppressed_updates() {
    let fixture = Fixture::new();
    fixture.create();
    fixture
        .repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER suppress_share_revoke BEFORE UPDATE OF status ON shares
             WHEN NEW.status = 'revoked' BEGIN SELECT RAISE(IGNORE); END;",
        )
        .expect("suppressing trigger");
    assert_eq!(
        fixture.repository.revoke_share(
            &fixture.share.id,
            &fixture.share.workspace_id,
            1_001,
            &event("share.revoked", &fixture.share, 1_001),
        ),
        Err(RepositoryError::Conflict)
    );
}
