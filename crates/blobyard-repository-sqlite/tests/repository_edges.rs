#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! `SQLite` repository failure-path regression coverage.

use blobyard_contract::{
    AuditValue, CredentialRepository, LocalApiTokenRecord, MetadataRepository, NewAuditEvent,
    NewDownloadGrant, NewObjectVersion, NewShare, NewUploadReservation, ProjectRecord,
    RepositoryError, SharingRepository, TransferRepository, WorkspaceRecord,
};
use blobyard_core::Slug;
use blobyard_repository_sqlite::SqliteRepository;
use rusqlite::Connection;
use tempfile::TempDir;

fn slug(value: &str) -> Slug {
    Slug::new(value.to_owned()).expect("fixture slug")
}

fn repository() -> (TempDir, SqliteRepository) {
    let temporary = TempDir::new().expect("temporary");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");
    repository
        .create_workspace(&WorkspaceRecord {
            id: "workspace_fixture".to_owned(),
            name: "Fixture".to_owned(),
            slug: slug("fixture"),
        })
        .expect("workspace");
    repository
        .create_project(&ProjectRecord {
            id: "project_fixture".to_owned(),
            workspace_id: "workspace_fixture".to_owned(),
            name: "Fixture".to_owned(),
            slug: slug("fixture"),
        })
        .expect("project");
    (temporary, repository)
}

fn audit() -> NewAuditEvent {
    NewAuditEvent {
        id: "audit_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "token_fixture".to_owned(),
        action: "api_token.revoked".to_owned(),
        request_id: "request_fixture".to_owned(),
        target_type: "api_token".to_owned(),
        metadata: vec![(
            "tokenId".to_owned(),
            AuditValue::String("missing".to_owned()),
        )],
        created_at_ms: 1,
    }
}

#[test]
fn opening_non_sqlite_content_returns_unavailable() {
    let temporary = TempDir::new().expect("temporary");
    let path = temporary.path().join("invalid.sqlite3");
    std::fs::write(&path, b"not a sqlite database").expect("invalid database fixture");

    assert!(matches!(
        SqliteRepository::open(&path),
        Err(RepositoryError::Unavailable)
    ));
}

fn reservation(id: &str) -> NewUploadReservation {
    NewUploadReservation {
        id: id.to_owned(),
        project_id: "project_fixture".to_owned(),
        object_path: format!("{id}.bin"),
        filename: format!("{id}.bin"),
        content_type: "application/octet-stream".to_owned(),
        expected_size: 4,
        expected_checksum: "00".repeat(32),
        storage_key: format!("versions/{id}"),
        capability_hash: if id == "upload_fixture" {
            "11".repeat(32)
        } else {
            "22".repeat(32)
        },
        expires_at_ms: 100,
        created_at_ms: 0,
        source: blobyard_contract::ObjectSource::Cli,
        git_repository: None,
        git_commit: None,
        git_branch: None,
        strategy: blobyard_contract::ReservationStrategy::Single,
        part_size: None,
        part_count: None,
    }
}

#[test]
fn namespace_and_token_edges_return_specific_failures() {
    let (_temporary, repository) = repository();
    assert_eq!(
        repository.install_bootstrap("invalid"),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.reserve_object_version(&NewObjectVersion {
            id: "version_zero".to_owned(),
            project_id: "project_fixture".to_owned(),
            object_path: "zero.bin".to_owned(),
            version: 0,
            storage_key: "versions/zero".to_owned(),
            source: blobyard_contract::ObjectSource::Cli,
            git_repository: None,
            git_commit: None,
            git_branch: None,
        }),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.complete_object_version("missing", 0, &"00".repeat(32)),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.revoke_api_token("missing", 1, &audit()),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn bootstrap_and_download_edges_return_specific_failures() {
    let (_temporary, repository) = repository();
    repository
        .install_bootstrap(&"22".repeat(32))
        .expect("bootstrap");
    let invalid_token = LocalApiTokenRecord {
        id: "token_invalid".to_owned(),
        name: "Invalid".to_owned(),
        token_prefix: "bya_invalid".to_owned(),
        secret_hash: "33".repeat(32),
        scopes: Vec::new(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: None,
        created_at_ms: 1,
        expires_at_ms: 2,
        last_used_at_ms: None,
        revoked_at_ms: None,
    };
    assert_eq!(
        repository.exchange_bootstrap(
            &"22".repeat(32),
            &invalid_token,
            &blobyard_contract::LocalCliSessionRecord {
                id: "session_invalid".to_owned(),
                token_id: invalid_token.id.clone(),
                workspace_id: invalid_token.workspace_id.clone(),
                name: invalid_token.name.clone(),
                platform: "test".to_owned(),
                version: "0.1.12".to_owned(),
                created_at_ms: invalid_token.created_at_ms,
                last_used_at_ms: None,
                revoked_at_ms: None,
            },
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.issue_download(&NewDownloadGrant {
            version_id: "missing".to_owned(),
            capability_hash: "44".repeat(32),
            expires_at_ms: 100,
        }),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.renew_upload("missing", 200),
        Err(RepositoryError::Conflict)
    );
}

#[test]
fn transfer_state_guards_reject_repeated_and_interrupted_transitions() {
    let (temporary, repository) = repository();
    let input = reservation("upload_fixture");
    repository.reserve_upload(&input).expect("reservation");
    assert_eq!(
        repository.complete_upload(&input.id),
        Err(RepositoryError::Conflict)
    );
    repository
        .record_uploaded_bytes(&input.id, 4, &input.expected_checksum)
        .expect("uploaded");
    assert_eq!(
        repository.record_uploaded_bytes(&input.id, 4, &input.expected_checksum),
        Err(RepositoryError::Conflict)
    );

    let interrupted = reservation("upload_interrupted");
    repository
        .reserve_upload(&interrupted)
        .expect("reservation");
    Connection::open(temporary.path().join("metadata.sqlite3"))
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER ignore_upload_update BEFORE UPDATE OF state ON upload_reservations WHEN NEW.state = 'uploaded' BEGIN SELECT RAISE(IGNORE); END;",
        )
        .expect("failure trigger");
    assert_eq!(
        repository.record_uploaded_bytes(
            &interrupted.id,
            interrupted.expected_size,
            &interrupted.expected_checksum,
        ),
        Err(RepositoryError::Conflict)
    );

    let mut aborted = reservation("upload_abort");
    aborted.capability_hash = "55".repeat(32);
    repository.reserve_upload(&aborted).expect("reservation");
    Connection::open(temporary.path().join("metadata.sqlite3"))
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER fail_abort BEFORE UPDATE OF state ON upload_reservations WHEN NEW.state = 'aborted' BEGIN SELECT RAISE(ABORT, 'fixture'); END;",
        )
        .expect("failure trigger");
    assert_eq!(
        repository.abort_upload(&aborted.id),
        Err(RepositoryError::Conflict)
    );
}

#[test]
fn share_edges_reject_invalid_audit_and_counter_overflow_through_the_public_adapter() {
    let temporary = TempDir::new().expect("temporary");
    let path = temporary.path().join("metadata.sqlite3");
    let repository = SqliteRepository::open(&path).expect("repository");
    blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
    blobyard_testkit::transfer_conformance(&repository, "project_fixture")
        .expect("transfer conformance");
    let share = NewShare {
        id: "share_edge".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        version_id: "upload_two".to_owned(),
        capability_hash: "e".repeat(64),
        expires_at_ms: 5_000,
        maximum_downloads: None,
        created_at_ms: 1_000,
    };
    assert_eq!(
        repository.create_share(
            &share,
            &blobyard_testkit::share_event("share.invalid", &share.id, 1_000),
        ),
        Err(RepositoryError::InvalidInput)
    );
    repository
        .create_share(
            &share,
            &blobyard_testkit::share_event("share.created", &share.id, 1_000),
        )
        .expect("share");
    Connection::open(path)
        .expect("connection")
        .execute(
            "UPDATE shares SET consumed_count = ?1 WHERE id = ?2",
            rusqlite::params![i64::MAX, share.id],
        )
        .expect("maximum count");
    assert_eq!(
        repository.issue_share_download(
            &share.capability_hash,
            1_001,
            &NewDownloadGrant {
                version_id: share.version_id,
                capability_hash: "f".repeat(64),
                expires_at_ms: 1_100,
            },
            &blobyard_testkit::share_event("share.download_issued", &share.id, 1_001),
        ),
        Err(RepositoryError::InvalidInput)
    );
}
