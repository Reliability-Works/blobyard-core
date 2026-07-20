#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    StageHooks, existing, existing_with, receive, receive_part, stage_body, stage_body_with, store,
    store_part,
};
use crate::api::AppState;
use axum::{
    body::{Body, Bytes},
    http::StatusCode,
    response::IntoResponse,
};
use blobyard_contract::{
    ByteRange, MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, ObjectVersionRecord,
    ReservationState, StorageError, StorageKey, StorageMetadata, StorageRead,
    UploadReservationRecord, UploadState,
};
use futures_util::stream;
use std::io::{Read, Write};
use std::sync::Arc;
use tempfile::{NamedTempFile, TempDir};

use crate::storage_get_macro::storage_read_methods;

#[path = "transfer_io_tests/multipart.rs"]
mod multipart;

#[derive(Clone)]
struct FixtureStorage {
    put: Result<StorageMetadata, StorageError>,
    head: Result<StorageMetadata, StorageError>,
    panic_on_put: bool,
    part: Result<MultipartPart, StorageError>,
    panic_on_part: bool,
}

impl ObjectStorage for FixtureStorage {
    fn put(
        &self,
        _key: &StorageKey,
        _source: &mut dyn Read,
        _expected: Option<&ObjectChecksum>,
    ) -> Result<StorageMetadata, StorageError> {
        assert!(!self.panic_on_put, "fixture put panic");
        self.put.clone()
    }

    storage_read_methods!(self, StorageError::NotFound, self.head.clone());

    fn delete(&self, _key: &StorageKey) -> Result<(), StorageError> {
        Err(StorageError::Conflict)
    }

    fn begin_multipart(
        &self,
        _key: &StorageKey,
        _expected: &StorageMetadata,
    ) -> Result<MultipartId, StorageError> {
        Err(StorageError::InvalidInput)
    }

    fn put_part(
        &self,
        upload: &MultipartId,
        number: u32,
        _source: &mut dyn Read,
    ) -> Result<MultipartPart, StorageError> {
        assert!(!self.panic_on_part, "fixture part panic");
        self.part.clone().map(|mut part| {
            part.number = number;
            assert_eq!(upload.0, "provider");
            part
        })
    }

    fn complete_multipart(
        &self,
        _upload: &MultipartId,
        _parts: &[MultipartPart],
    ) -> Result<StorageMetadata, StorageError> {
        Err(StorageError::Conflict)
    }

    fn abort_multipart(&self, _upload: &MultipartId) -> Result<(), StorageError> {
        Err(StorageError::InvalidInput)
    }
}

fn checksum() -> ObjectChecksum {
    ObjectChecksum::new("00".repeat(32)).expect("fixture checksum")
}

fn metadata(size: u64) -> StorageMetadata {
    StorageMetadata {
        size,
        checksum: checksum(),
    }
}

fn fixture_storage(
    put: Result<StorageMetadata, StorageError>,
    head: Result<StorageMetadata, StorageError>,
) -> FixtureStorage {
    FixtureStorage {
        put,
        head,
        panic_on_put: false,
        part: Err(StorageError::NotFound),
        panic_on_part: false,
    }
}

fn state(root: &TempDir, staging_directory: std::path::PathBuf) -> AppState {
    crate::test_support::filesystem_state(root, staging_directory)
}

fn reservation(storage_key: &str, checksum: &str, expected_size: u64) -> UploadReservationRecord {
    UploadReservationRecord {
        id: "upload_fixture".to_owned(),
        version: ObjectVersionRecord {
            id: "version_fixture".to_owned(),
            project_id: "project_fixture".to_owned(),
            object_path: "fixture.bin".to_owned(),
            version: 1,
            storage_key: storage_key.to_owned(),
            state: UploadState::Pending,
            size: None,
            checksum: None,
            created_at_ms: 0,
            source: blobyard_contract::ObjectSource::Cli,
            git_repository: None,
            git_commit: None,
            git_branch: None,
        },
        filename: "fixture.bin".to_owned(),
        content_type: "application/octet-stream".to_owned(),
        expected_size,
        expected_checksum: checksum.to_owned(),
        expires_at_ms: 1,
        state: ReservationState::Requested,
        strategy: blobyard_contract::ReservationStrategy::Single,
        part_size: None,
        part_count: None,
        provider_upload_id: None,
    }
}

#[tokio::test]
async fn body_staging_accepts_exact_bytes_and_rejects_bad_streams_and_sizes() {
    let root = TempDir::new().expect("root");
    let staging = root.path().join("staging");
    std::fs::create_dir(&staging).expect("staging");
    let state = state(&root, staging);
    let staged = stage_body(&state, 4, Body::from("data"))
        .await
        .expect("exact body");
    assert_eq!(std::fs::read(staged.path()).expect("staged bytes"), b"data");
    assert!(stage_body(&state, 3, Body::from("data")).await.is_err());
    assert!(stage_body(&state, 5, Body::from("data")).await.is_err());

    let body = Body::from_stream(stream::once(async {
        Err::<Bytes, std::io::Error>(std::io::Error::other("fixture failure"))
    }));
    assert!(stage_body(&state, 0, body).await.is_err());
}

#[tokio::test]
async fn body_staging_and_receive_reject_unusable_paths_and_metadata() {
    let root = TempDir::new().expect("root");
    let staging_file = root.path().join("not-a-directory");
    std::fs::write(&staging_file, b"fixture").expect("blocker");
    let blocked = state(&root, staging_file);
    assert!(stage_body(&blocked, 0, Body::empty()).await.is_err());

    let root = TempDir::new().expect("root");
    let staging = root.path().join("staging");
    std::fs::create_dir(&staging).expect("staging");
    let state = state(&root, staging);
    assert!(
        receive(
            &state,
            &reservation("../invalid", &"00".repeat(32), 0),
            Body::empty()
        )
        .await
        .is_err()
    );
    assert!(
        receive(
            &state,
            &reservation("valid/key", "invalid", 0),
            Body::empty()
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn body_staging_maps_each_file_operation_failure() {
    let root = TempDir::new().expect("root");
    let staging = root.path().join("staging");
    std::fs::create_dir(&staging).expect("staging");
    let state = state(&root, staging);
    for hooks in [
        StageHooks {
            reopen: fail_reopen,
            ..StageHooks::PRODUCTION
        },
        StageHooks {
            after_write: fail_io,
            ..StageHooks::PRODUCTION
        },
        StageHooks {
            after_flush: fail_io,
            ..StageHooks::PRODUCTION
        },
        StageHooks {
            after_sync: fail_io,
            ..StageHooks::PRODUCTION
        },
    ] {
        assert!(
            stage_body_with(&state, 4, Body::from("data"), hooks)
                .await
                .is_err()
        );
    }
}

fn fail_reopen(_temporary: &NamedTempFile) -> std::io::Result<std::fs::File> {
    Err(std::io::Error::other("fixture reopen failure"))
}

fn fail_io(result: std::io::Result<()>) -> std::io::Result<()> {
    result.and_then(|()| Err(std::io::Error::other("fixture I/O failure")))
}

#[tokio::test]
async fn receive_conceals_a_panicking_blocking_storage_task() {
    let root = TempDir::new().expect("root");
    let staging = root.path().join("staging");
    std::fs::create_dir(&staging).expect("staging");
    let storage = Arc::new(FixtureStorage {
        put: Ok(metadata(4)),
        head: Ok(metadata(4)),
        panic_on_put: true,
        part: Err(StorageError::NotFound),
        panic_on_part: false,
    });
    let state = crate::test_support::state(&root, staging, storage);

    let error = receive(
        &state,
        &reservation("valid/key", &"00".repeat(32), 4),
        Body::from("data"),
    )
    .await
    .expect_err("panicking storage task");
    assert_eq!(
        error.into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn storage_commit_replays_identical_conflicts_and_rejects_drift() {
    let key = StorageKey::new("valid/key").expect("key");
    let checksum = checksum();
    let mut temporary = NamedTempFile::new().expect("temporary");
    temporary.write_all(b"data").expect("bytes");

    let success = fixture_storage(Ok(metadata(4)), Err(StorageError::Unavailable));
    assert_eq!(
        store(&success, &key, &checksum, &temporary).expect("put"),
        metadata(4)
    );

    let replay = fixture_storage(Err(StorageError::Conflict), Ok(metadata(4)));
    assert_eq!(
        store(&replay, &key, &checksum, &temporary).expect("replay"),
        metadata(4)
    );

    let drift = fixture_storage(Err(StorageError::Conflict), Ok(metadata(3)));
    assert_eq!(
        store(&drift, &key, &checksum, &temporary),
        Err(StorageError::IntegrityMismatch)
    );

    let missing = fixture_storage(Err(StorageError::Conflict), Err(StorageError::NotFound));
    assert_eq!(
        store(&missing, &key, &checksum, &temporary),
        Err(StorageError::NotFound)
    );

    let unavailable = fixture_storage(Err(StorageError::Unavailable), Ok(metadata(4)));
    assert_eq!(
        store(&unavailable, &key, &checksum, &temporary),
        Err(StorageError::Unavailable)
    );

    let removed = NamedTempFile::new().expect("removed temporary");
    let removed_path = removed.path().to_owned();
    std::fs::remove_file(&removed_path).expect("remove path");
    assert_eq!(
        store(&success, &key, &checksum, &removed),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        existing_with(&replay, &key, &checksum, &removed, fail_metadata),
        Err(StorageError::Unavailable)
    );
    assert!(!removed_path.exists());

    let different_checksum = ObjectChecksum::new("11".repeat(32)).expect("checksum");
    assert_eq!(
        existing(&replay, &key, &different_checksum, &temporary),
        Err(StorageError::IntegrityMismatch)
    );
}

fn fail_metadata(_temporary: &NamedTempFile) -> std::io::Result<std::fs::Metadata> {
    Err(std::io::Error::other("fixture metadata failure"))
}
