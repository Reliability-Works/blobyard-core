#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use crate::test_support::multipart_upload;
use axum::{http::StatusCode, response::IntoResponse};
use blobyard_api_client::CompletedPart;

fn upload() -> UploadReservationRecord {
    multipart_upload::record("upload_unit", 3, 3, 1, Some("provider"))
}

fn stored_part() -> UploadPartRecord {
    UploadPartRecord {
        upload_id: "upload_unit".to_owned(),
        part_number: 1,
        expected_size: 3,
        expires_at_ms: 10,
        received_size: Some(3),
        received_checksum: Some("b".repeat(64)),
        provider_tag: Some("provider-tag".to_owned()),
    }
}

fn submitted() -> CompletedPart {
    CompletedPart {
        part_number: 1,
        etag: format!("\"{}\"", "b".repeat(64)),
    }
}

fn status(error: ApiError) -> StatusCode {
    error.into_response().status()
}

#[test]
fn completion_parts_reject_every_inconsistent_client_and_durable_field() {
    let mut reservation = upload();
    reservation.part_count = None;
    assert_eq!(
        completion_parts(&reservation, &[submitted()], &[stored_part()])
            .expect_err("missing count")
            .into_response()
            .status(),
        StatusCode::CONFLICT
    );
    reservation.part_count = Some(1);
    assert_eq!(
        status(completion_parts(&reservation, &[], &[]).expect_err("missing parts")),
        StatusCode::CONFLICT
    );
    reservation.part_count = Some(10_001);
    assert_eq!(
        status(
            completion_parts(&reservation, &[submitted()], &[stored_part()])
                .expect_err("oversized part count")
        ),
        StatusCode::CONFLICT
    );
    reservation.part_count = Some(1);

    let mut cases = Vec::new();
    let mut record = stored_part();
    record.received_checksum = None;
    cases.push((submitted(), record, StatusCode::CONFLICT));
    let mut record = stored_part();
    record.received_size = None;
    cases.push((submitted(), record, StatusCode::CONFLICT));
    let mut client = submitted();
    client.part_number = 2;
    cases.push((client, stored_part(), StatusCode::BAD_REQUEST));
    let mut record = stored_part();
    record.part_number = 2;
    cases.push((submitted(), record, StatusCode::BAD_REQUEST));
    let mut client = submitted();
    client.etag = "\"wrong\"".to_owned();
    cases.push((client, stored_part(), StatusCode::BAD_REQUEST));
    let mut record = stored_part();
    record.received_size = Some(2);
    cases.push((submitted(), record, StatusCode::BAD_REQUEST));
    let mut record = stored_part();
    record.received_checksum = Some("invalid".to_owned());
    let mut client = submitted();
    client.etag = "\"invalid\"".to_owned();
    cases.push((client, record, StatusCode::BAD_REQUEST));

    for (client, record, expected) in cases {
        assert_eq!(
            status(
                completion_parts(&reservation, &[client], &[record])
                    .expect_err("inconsistent completion")
            ),
            expected
        );
    }
}

#[test]
fn provider_and_integrity_helpers_cover_success_conflict_and_storage_failures() {
    let mut reservation = upload();
    assert!(require_provider(reservation.clone()).is_ok());
    reservation.provider_upload_id = None;
    assert_eq!(
        status(require_provider(reservation).expect_err("missing provider")),
        StatusCode::CONFLICT
    );

    let metadata = StorageMetadata {
        size: 3,
        checksum: ObjectChecksum::new("a".repeat(64)).expect("checksum"),
    };
    assert!(verify_complete(&upload(), &metadata).is_ok());
    let mut wrong_size = metadata;
    wrong_size.size = 2;
    assert_eq!(
        status(verify_complete(&upload(), &wrong_size).expect_err("wrong size")),
        StatusCode::BAD_REQUEST
    );
    let wrong_checksum = StorageMetadata {
        size: 3,
        checksum: ObjectChecksum::new("c".repeat(64)).expect("checksum"),
    };
    assert_eq!(
        status(verify_complete(&upload(), &wrong_checksum).expect_err("wrong checksum")),
        StatusCode::BAD_REQUEST
    );

    assert!(ignore_missing(Ok(())).is_ok());
    assert!(ignore_missing(Err(StorageError::NotFound)).is_ok());
    assert_eq!(
        status(ignore_missing(Err(StorageError::Unavailable)).expect_err("storage failure")),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn provider_initialization_is_idempotent_for_single_existing_and_racing_uploads() {
    let fixture = crate::transfers::test_seams::fixture(&["object:write"]);
    let mut single = upload();
    single.strategy = ReservationStrategy::Single;
    single.provider_upload_id = None;
    assert_eq!(
        ensure_provider(&fixture.state, single.clone()).expect("single provider"),
        single
    );
    assert_eq!(
        ensure_provider(&fixture.state, upload()).expect("existing provider"),
        upload()
    );

    let mut race = multipart_upload::record("upload_race", 3, 3, 1, None);
    race.version.project_id = fixture.project.id.clone();
    let mut input = multipart_upload::reservation(&race, 'b', 10);
    let stale = fixture
        .state
        .repository
        .reserve_upload(&input)
        .expect("reservation");
    fixture
        .state
        .repository
        .attach_multipart(&input.id, "provider-existing")
        .expect("existing provider");
    let attached = ensure_provider(&fixture.state, stale).expect("racing provider");
    assert_eq!(
        attached.provider_upload_id.as_deref(),
        Some("provider-existing")
    );

    input.storage_key = "../invalid".to_owned();
    let mut invalid = attached;
    invalid.provider_upload_id = None;
    invalid.version.storage_key = input.storage_key;
    assert_eq!(
        status(ensure_provider(&fixture.state, invalid).expect_err("invalid key")),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let mut invalid_checksum = upload();
    invalid_checksum.provider_upload_id = None;
    invalid_checksum.expected_checksum = "invalid".to_owned();
    assert_eq!(
        status(
            ensure_provider(&fixture.state, invalid_checksum)
                .expect_err("invalid expected checksum"),
        ),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn single_upload_completion_and_status_reject_multipart_fields() {
    let fixture = crate::transfers::test_seams::fixture(&["object:write"]);
    let mut single = upload();
    single.strategy = ReservationStrategy::Single;
    assert!(complete(&fixture.state, &single, &[]).is_ok());
    assert_eq!(
        status(complete(&fixture.state, &single, &[submitted()]).expect_err("parts on single")),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        completed_part_numbers(&fixture.state, &single).expect("single status"),
        Vec::<u32>::new()
    );
}
