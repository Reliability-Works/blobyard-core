#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use crate::Repository;
use crate::repository_fault_tests::FaultingRepository;
use crate::test_support::multipart_storage::MultipartStorage;
use crate::test_support::multipart_upload;
use axum::{http::StatusCode, response::IntoResponse};
use blobyard_api_client::CompletedPart;
use blobyard_contract::{NewUploadPartGrant, NewUploadReservation};
use std::sync::Arc;

fn status(error: ApiError) -> StatusCode {
    error.into_response().status()
}

fn upload() -> UploadReservationRecord {
    multipart_upload::record("upload_failure", 3, 3, 1, None)
}

fn input() -> NewUploadReservation {
    multipart_upload::reservation(&upload(), 'c', 10)
}

fn part() -> NewUploadPartGrant {
    NewUploadPartGrant {
        upload_id: "upload_failure".to_owned(),
        part_number: 1,
        expected_size: 3,
        capability_hash: "d".repeat(64),
        expires_at_ms: 10,
    }
}

fn metadata() -> StorageMetadata {
    StorageMetadata {
        size: 3,
        checksum: ObjectChecksum::new("a".repeat(64)).expect("checksum"),
    }
}

fn seeded(
    storage: MultipartStorage,
) -> (
    crate::transfers::test_seams::TransferFixture,
    AppState,
    UploadReservationRecord,
) {
    let fixture = crate::transfers::test_seams::fixture(&["object:write"]);
    fixture
        .state
        .repository
        .reserve_upload(&input())
        .expect("reservation");
    fixture
        .state
        .repository
        .attach_multipart("upload_failure", "provider")
        .expect("provider");
    fixture
        .state
        .repository
        .issue_upload_parts(&[part()])
        .expect("part");
    fixture
        .state
        .repository
        .record_uploaded_part(
            "upload_failure",
            1,
            3,
            &"b".repeat(64),
            Some("provider-tag"),
        )
        .expect("uploaded part");
    let upload = fixture
        .state
        .repository
        .upload_by_id("upload_failure")
        .expect("upload");
    let mut state = fixture.state.clone();
    state.storage = Arc::new(storage);
    (fixture, state, upload)
}

fn submitted() -> CompletedPart {
    CompletedPart {
        part_number: 1,
        etag: format!("\"{}\"", "b".repeat(64)),
    }
}

#[test]
fn provider_initialization_maps_storage_repository_and_cleanup_failures() {
    let fixture = crate::transfers::test_seams::fixture(&["object:write"]);
    let mut state = fixture.state.clone();
    state.storage = Arc::new(MultipartStorage::unavailable());
    assert_eq!(
        status(ensure_provider(&state, upload()).expect_err("begin failure")),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    for abort in [Ok(()), Err(StorageError::Unavailable)] {
        let mut storage = MultipartStorage::unavailable();
        storage.begin = Ok(MultipartId("created".to_owned()));
        storage.abort = abort;
        let mut state = fixture.state.clone();
        let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
        state.repository = Arc::new(FaultingRepository::new(inner, 0));
        state.storage = Arc::new(storage);
        assert_eq!(
            status(ensure_provider(&state, upload()).expect_err("attach failure")),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[test]
fn completion_maps_repository_provider_fallback_and_integrity_failures() {
    let storage = MultipartStorage::unavailable();
    let (_fixture, state, upload) = seeded(storage);
    assert_eq!(
        status(complete(&state, &upload, &[submitted()]).expect_err("provider failure")),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let mut fallback = MultipartStorage::unavailable();
    fallback.complete = Err(StorageError::NotFound);
    fallback.head = Ok(metadata());
    let (_fixture, state, upload) = seeded(fallback);
    complete(&state, &upload, &[submitted()]).expect("fallback completion");

    let mut missing = MultipartStorage::unavailable();
    missing.complete = Err(StorageError::NotFound);
    let (_fixture, state, upload) = seeded(missing);
    assert_eq!(
        status(complete(&state, &upload, &[submitted()]).expect_err("head failure")),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let mut mismatched = MultipartStorage::unavailable();
    mismatched.complete = Ok(StorageMetadata {
        size: 2,
        checksum: ObjectChecksum::new("a".repeat(64)).expect("checksum"),
    });
    let (_fixture, state, upload) = seeded(mismatched);
    assert_eq!(
        status(complete(&state, &upload, &[submitted()]).expect_err("provider mismatch")),
        StatusCode::BAD_REQUEST
    );

    let mut no_provider = upload.clone();
    no_provider.provider_upload_id = None;
    assert_eq!(
        status(complete(&state, &no_provider, &[submitted()]).expect_err("missing provider")),
        StatusCode::CONFLICT
    );

    let mut invalid = upload;
    invalid.version.storage_key = "../invalid".to_owned();
    assert_eq!(
        status(existing_metadata(&state, &invalid).expect_err("invalid key")),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn completion_and_abort_propagate_repository_and_provider_failures() {
    let mut storage = MultipartStorage::unavailable();
    storage.complete = Ok(metadata());
    let (_fixture, mut state, upload) = seeded(storage);
    let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
    state.repository = Arc::new(FaultingRepository::new(inner, 0));
    assert_eq!(
        status(complete(&state, &upload, &[submitted()]).expect_err("list failure")),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let mut state = state;
    let mut storage = MultipartStorage::unavailable();
    storage.abort = Err(StorageError::Unavailable);
    state.storage = Arc::new(storage);
    assert_eq!(
        status(abort_storage(&state, &upload).expect_err("abort failure")),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
