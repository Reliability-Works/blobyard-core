#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{assert_error, fixture, request, router, send_json};
use crate::api::AppState;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use blobyard_contract::{ObjectSource, ReservationStrategy, StorageKey, UploadReservationRecord};
use blobyard_core::SecretString;
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};
use std::io::Read;
use tower::ServiceExt;

fn checksum(bytes: &[u8]) -> String {
    blobyard_core::hex_digest(&Sha256::digest(bytes))
}

fn seed_multipart(
    state: &AppState,
    project: &blobyard_contract::ProjectRecord,
) -> UploadReservationRecord {
    let mut request = request("multipart.bin");
    request.size_bytes = 5;
    request.checksum_sha256 = checksum(b"abcde");
    let capability = SecretString::new("whole-upload-capability").expect("capability");
    let mut input = crate::transfer_grants::reservation_input(
        &request,
        project,
        "upload_multipart",
        &capability,
        2_000_000_000_000,
        ObjectSource::Cli,
    );
    input.strategy = ReservationStrategy::Multipart;
    input.part_size = Some(3);
    input.part_count = Some(2);
    let reservation = state
        .repository
        .reserve_upload(&input)
        .expect("multipart reservation");
    crate::transfer_multipart::ensure_provider(state, reservation).expect("provider upload")
}

async fn send_raw(state: &AppState, method: &str, path: &str, bytes: &[u8]) -> Response {
    router(state)
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(header::AUTHORIZATION, "Bearer secret")
                .body(Body::from(bytes.to_vec()))
                .expect("request"),
        )
        .await
        .expect("response")
}

async fn json(response: Response) -> serde_json::Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("JSON")
}

async fn part_grants(state: &AppState, numbers: &[u32]) -> serde_json::Value {
    let response = send_json(
        state,
        "POST",
        "/v1/uploads/parts/request",
        serde_json::json!({
            "uploadId": "upload_multipart",
            "partNumbers": numbers
        }),
        None,
    )
    .await;
    let status = response.status();
    let value = json(response).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}

async fn put_part(state: &AppState, path: &str, bytes: &[u8]) -> String {
    let response = send_raw(state, "PUT", path, bytes).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    response
        .headers()
        .get(header::ETAG)
        .expect("part ETag")
        .to_str()
        .expect("part ETag text")
        .to_owned()
}

async fn assert_resume(state: &AppState, first_path: &str) {
    let status = send_raw(
        state,
        "GET",
        "/v1/uploads/status?uploadId=upload_multipart",
        b"",
    )
    .await;
    assert_eq!(
        json(status).await["data"]["completedParts"],
        serde_json::json!([1])
    );
    assert_eq!(part_path(&part_grants(state, &[1]).await, 0), first_path);
}

async fn assert_invalid_completion(state: &AppState, first_etag: &str) {
    let response = send_json(
        state,
        "POST",
        "/v1/uploads/complete",
        serde_json::json!({
            "uploadId": "upload_multipart",
            "parts": [
                {"partNumber": 1, "etag": first_etag},
                {"partNumber": 2, "etag": "\"wrong\""}
            ]
        }),
        None,
    )
    .await;
    assert_error(
        response,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
        "That request isn't valid. Check the command and try again.",
    )
    .await;
}

async fn complete(state: &AppState, first_etag: &str, second_etag: &str) {
    let response = send_json(
        state,
        "POST",
        "/v1/uploads/complete",
        serde_json::json!({
            "uploadId": "upload_multipart",
            "parts": [
                {"partNumber": 1, "etag": first_etag},
                {"partNumber": 2, "etag": second_etag}
            ]
        }),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let value = json(response).await;
    assert_eq!(value["data"]["sizeBytes"], 5);
    assert_eq!(value["data"]["checksumSha256"], checksum(b"abcde"));
}

fn assert_stored_bytes(state: &AppState, reservation: UploadReservationRecord) {
    let key = StorageKey::new(reservation.version.storage_key).expect("storage key");
    let mut stored = state.storage.get(&key, None).expect("stored object");
    let mut bytes = Vec::new();
    stored.reader.read_to_end(&mut bytes).expect("stored bytes");
    assert_eq!(bytes, b"abcde");
}

fn part_path(response: &serde_json::Value, index: usize) -> &str {
    response["data"]["parts"][index]["uploadUrl"]
        .as_str()
        .expect("part URL")
        .strip_prefix("http://127.0.0.1:8787")
        .expect("part path")
}

#[tokio::test]
async fn upload_strategy_switches_at_the_exact_limit_and_rejects_excess_parts() {
    let (_root, state, _project) = fixture();
    for (idempotency, size, strategy) in [
        ("single-limit", 100 * 1_024 * 1_024, "single"),
        ("multipart-limit", 100 * 1_024 * 1_024 + 1, "multipart"),
    ] {
        let mut value = request("strategy.bin");
        value.size_bytes = size;
        let response = send_json(
            &state,
            "POST",
            "/v1/uploads/request",
            serde_json::to_value(value).expect("request value"),
            Some(idempotency),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json(response).await["data"]["strategy"], strategy);
    }

    let mut too_large = request("too-large.bin");
    too_large.size_bytes = 16 * 1_024 * 1_024 * 10_000 + 1;
    assert_error(
        send_json(
            &state,
            "POST",
            "/v1/uploads/request",
            serde_json::to_value(too_large).expect("request value"),
            Some("too-many-parts"),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
        "That request isn't valid. Check the command and try again.",
    )
    .await;
}

#[tokio::test]
async fn multipart_upload_resumes_and_completes_with_exact_etags_and_bytes() {
    let (_root, state, project) = fixture();
    let reservation = seed_multipart(&state, &project);
    let grants = part_grants(&state, &[1, 2]).await;
    let first_path = part_path(&grants, 0);
    let second_path = part_path(&grants, 1);
    let first_etag = put_part(&state, first_path, b"abc").await;
    assert_resume(&state, first_path).await;
    let second_etag = put_part(&state, second_path, b"de").await;
    assert_invalid_completion(&state, &first_etag).await;
    complete(&state, &first_etag, &second_etag).await;
    assert_stored_bytes(&state, reservation);
}

#[path = "transfer_multipart_security_tests.rs"]
mod security;
