#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use crate::Repository;
use crate::auth::Principal;
use crate::inbox_upload_auth::UploadAuthority;
use crate::repository_fault_tests::FaultingRepository;
use crate::test_support::multipart_upload;
use axum::{http::StatusCode, response::IntoResponse};
use blobyard_contract::{NewUploadPartGrant, NewUploadReservation};
use blobyard_core::SecretString;
use std::sync::Arc;

fn upload() -> UploadReservationRecord {
    multipart_upload::record("upload_http_unit", 5, 3, 2, Some("provider"))
}

fn status(error: ApiError) -> StatusCode {
    error.into_response().status()
}

fn error<T>(result: Result<T, ApiError>) -> ApiError {
    result.err().expect("expected multipart grant failure")
}

fn input() -> NewUploadReservation {
    multipart_upload::reservation(&upload(), 'c', i64::MAX as u64)
}

fn seeded_fixture() -> crate::transfers::test_seams::TransferFixture {
    let fixture = crate::transfers::test_seams::fixture(&["object:write"]);
    let reservation = fixture
        .state
        .repository
        .reserve_upload(&input())
        .expect("reservation");
    crate::transfer_multipart::ensure_provider(&fixture.state, reservation).expect("provider");
    fixture
}

#[test]
fn multipart_grants_reject_every_invalid_reservation_and_batch_shape() {
    let fixture = crate::transfers::test_seams::fixture(&["object:write"]);
    let mut reservation = upload();
    reservation.part_count = None;
    assert_eq!(
        status(error(build_grants(&fixture.state, &reservation, &[1], 1))),
        StatusCode::CONFLICT
    );
    reservation = upload();
    reservation.part_size = None;
    assert_eq!(
        status(error(build_grants(&fixture.state, &reservation, &[1], 1))),
        StatusCode::CONFLICT
    );
    let mut wrong_strategy = upload();
    wrong_strategy.strategy = ReservationStrategy::Single;
    let mut missing_provider = upload();
    missing_provider.provider_upload_id = None;
    let mut expired = upload();
    expired.expires_at_ms = 1;
    for value in [wrong_strategy, missing_provider, expired] {
        assert_eq!(
            status(error(build_grants(&fixture.state, &value, &[1], 1))),
            StatusCode::CONFLICT
        );
    }
    for numbers in [vec![], vec![0], vec![3], vec![1, 1], vec![1; 101]] {
        assert_eq!(
            status(error(build_grants(&fixture.state, &upload(), &numbers, 1,))),
            StatusCode::BAD_REQUEST
        );
    }
}

#[test]
fn multipart_grants_use_exact_final_part_size_and_fail_closed_on_bad_origin() {
    let fixture = crate::transfers::test_seams::fixture(&["object:write"]);
    let grants = build_grants(&fixture.state, &upload(), &[1, 2], 1).expect("part grants");
    assert_eq!(grants[0].input.expected_size, 3);
    assert_eq!(grants[1].input.expected_size, 2);

    let mut state = fixture.state;
    state.public_origin = "invalid\norigin".to_owned();
    assert_eq!(
        status(error(build_grants(&state, &upload(), &[1], 1))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn multipart_etag_header_rejects_non_header_text() {
    assert_eq!(
        status(etag_header("invalid\nchecksum").expect_err("invalid ETag")),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        status(part_response("invalid\nchecksum").expect_err("invalid response ETag")),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

fn parts_request() -> RequestUploadPartsRequest {
    RequestUploadPartsRequest {
        upload_id: "upload_http_unit".to_owned(),
        part_numbers: vec![1],
    }
}

#[test]
fn multipart_parts_response_propagates_expiry_formatting_failure() {
    let fixture = crate::transfers::test_seams::fixture(&["object:write"]);
    let grants = build_grants(&fixture.state, &upload(), &[1], 1).expect("grant");
    assert_eq!(
        status(error(parts_response(grants, u64::MAX))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn request_parts_propagates_clock_and_repository_failures() {
    let fixture = seeded_fixture();
    let clock = request_parts_at(
        State(fixture.state),
        UploadAuthority::Operator(Principal(fixture.principal)),
        Ok(Json(parts_request())),
        Err(ApiError::internal()),
    );
    assert_eq!(status(error(clock)), StatusCode::INTERNAL_SERVER_ERROR);

    for failure_index in [2, 3] {
        let fixture = seeded_fixture();
        let mut state = fixture.state;
        let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
        state.repository = Arc::new(FaultingRepository::new(inner, failure_index));
        let result = request_parts_at(
            State(state),
            UploadAuthority::Operator(Principal(fixture.principal)),
            Ok(Json(parts_request())),
            Ok(1),
        );
        assert_eq!(status(error(result)), StatusCode::INTERNAL_SERVER_ERROR);
    }
}

#[tokio::test]
async fn request_parts_propagates_audit_failure_after_durable_grant_issuance() {
    let fixture = seeded_fixture();
    let mut state = fixture.state.clone();
    let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
    state.repository = Arc::new(FaultingRepository::new(inner, 4));
    let result = request_parts(
        State(state),
        UploadAuthority::Operator(Principal(fixture.principal.clone())),
        Ok(Json(parts_request())),
    )
    .await;
    assert_eq!(status(error(result)), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn put_part_conceals_capability_and_clock_failures() {
    let fixture = seeded_fixture();
    let invalid = put_part(
        State(fixture.state.clone()),
        Path(String::new()),
        Body::empty(),
    )
    .await;
    assert_eq!(status(error(invalid)), StatusCode::NOT_FOUND);

    let capability = SecretString::new("missing-part-capability").expect("capability");
    let clock = put_part_at(
        &fixture.state,
        &capability,
        Body::empty(),
        Err(ApiError::internal()),
    )
    .await;
    assert_eq!(status(error(clock)), StatusCode::INTERNAL_SERVER_ERROR);

    let missing = put_part_at(&fixture.state, &capability, Body::empty(), Ok(1)).await;
    assert_eq!(status(error(missing)), StatusCode::NOT_FOUND);

    let mut state = fixture.state.clone();
    let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
    state.repository = Arc::new(FaultingRepository::new(inner, 0));
    let unavailable = put_part_at(&state, &capability, Body::empty(), Ok(1)).await;
    assert_eq!(
        status(error(unavailable)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn put_part_propagates_the_durable_record_failure() {
    let fixture = seeded_fixture();
    let raw = SecretString::new("raw-part-capability").expect("capability");
    fixture
        .state
        .repository
        .issue_upload_parts(&[NewUploadPartGrant {
            upload_id: "upload_http_unit".to_owned(),
            part_number: 1,
            expected_size: 3,
            capability_hash: hash(raw.expose_secret()),
            expires_at_ms: i64::MAX as u64,
        }])
        .expect("part grant");
    let mut state = fixture.state.clone();
    let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
    state.repository = Arc::new(FaultingRepository::new(inner, 1));
    let result = put_part(
        State(state),
        Path(raw.expose_secret().to_owned()),
        Body::from("abc"),
    )
    .await;
    assert_eq!(status(error(result)), StatusCode::INTERNAL_SERVER_ERROR);
}
