#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::contract_test_support::{assert_error, response_json, send};
use axum::http::StatusCode;
use blobyard_server::transfers::test_seams;

fn upload_request(filename: &str, content_type: &str) -> serde_json::Value {
    serde_json::json!({
        "workspace": "fixture",
        "project": "project",
        "path": "object.bin",
        "filename": filename,
        "sizeBytes": 1,
        "checksumSha256": "00".repeat(32),
        "contentType": content_type
    })
}

async fn reserve_upload(fixture: &test_seams::TransferFixture) -> serde_json::Value {
    let request = serde_json::to_vec(&upload_request("object.bin", "application/octet-stream"))
        .expect("request JSON");
    let response = send(fixture, "POST", "/v1/uploads/request", &request, true).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

async fn reserve_multipart(
    fixture: &test_seams::TransferFixture,
    filename: &str,
) -> serde_json::Value {
    let mut request = upload_request(filename, "application/octet-stream");
    request["sizeBytes"] = serde_json::json!(100 * 1_024 * 1_024 + 1);
    let body = serde_json::to_vec(&request).expect("request JSON");
    let response = send(fixture, "POST", "/v1/uploads/request", &body, true).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

#[tokio::test]
async fn oversized_uploads_return_the_multipart_contract() {
    let fixture = test_seams::fixture(&["object:write"]);
    let value = reserve_multipart(&fixture, "multipart.bin").await;
    assert_eq!(value["data"]["strategy"], "multipart");
    assert!(value["data"]["uploadUrl"].is_null());
    assert_eq!(value["data"]["partSizeBytes"], 16 * 1_024 * 1_024);
}

#[tokio::test]
async fn multipart_status_fails_closed_when_part_storage_is_unavailable() {
    let fixture = test_seams::fixture(&["object:write"]);
    let value = reserve_multipart(&fixture, "multipart-status.bin").await;
    let upload_id = value["data"]["uploadId"]
        .as_str()
        .expect("upload identifier");
    fixture.break_upload_parts();
    assert_error(
        send(
            &fixture,
            "GET",
            &format!("/v1/uploads/status?uploadId={upload_id}"),
            b"",
            false,
        )
        .await,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    )
    .await;
}

#[tokio::test]
async fn transfer_handlers_enforce_each_required_scope() {
    let fixture = test_seams::fixture(&["fixture"]);
    let request = serde_json::to_vec(&upload_request("object.bin", "application/octet-stream"))
        .expect("request JSON");
    for (method, path, body, idempotency) in [
        ("POST", "/v1/uploads/request", request.as_slice(), true),
        (
            "POST",
            "/v1/uploads/parts/request",
            br#"{"uploadId":"missing","partNumbers":[1]}"#.as_slice(),
            false,
        ),
        (
            "POST",
            "/v1/uploads/complete",
            br#"{"uploadId":"missing","parts":[]}"#.as_slice(),
            false,
        ),
        (
            "POST",
            "/v1/uploads/abort",
            br#"{"uploadId":"missing"}"#.as_slice(),
            false,
        ),
        (
            "GET",
            "/v1/uploads/status?uploadId=missing",
            b"".as_slice(),
            false,
        ),
    ] {
        assert_error(
            send(&fixture, method, path, body, idempotency).await,
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        )
        .await;
    }
}

#[tokio::test]
async fn upload_status_uses_upload_authority_not_download_authority() {
    let path = "/v1/uploads/status?uploadId=missing";
    let read_only = test_seams::fixture(&["object:read"]);
    assert_error(
        send(&read_only, "GET", path, b"", false).await,
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
    )
    .await;

    let writer = test_seams::fixture(&["object:write"]);
    assert_error(
        send(&writer, "GET", path, b"", false).await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
}

#[tokio::test]
async fn transfer_handlers_reject_malformed_json_and_invalid_fields() {
    let fixture = test_seams::fixture(&["object:read", "object:write"]);
    for path in [
        "/v1/uploads/request",
        "/v1/uploads/parts/request",
        "/v1/uploads/complete",
        "/v1/uploads/abort",
    ] {
        assert_error(
            send(&fixture, "POST", path, b"{", true).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
    for request in [
        upload_request("", "application/octet-stream"),
        upload_request("object.bin", ""),
    ] {
        let body = serde_json::to_vec(&request).expect("request JSON");
        assert_error(
            send(&fixture, "POST", "/v1/uploads/request", &body, true).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
    let mut invalid_path = upload_request("object.bin", "application/octet-stream");
    invalid_path["path"] = serde_json::json!("/absolute");
    let body = serde_json::to_vec(&invalid_path).expect("request JSON");
    assert_error(
        send(&fixture, "POST", "/v1/uploads/request", &body, true).await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
    let body = serde_json::to_vec(&upload_request("object.bin", "application/octet-stream"))
        .expect("request JSON");
    assert_error(
        send(&fixture, "POST", "/v1/uploads/request", &body, false).await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
    assert_error(
        send(
            &fixture,
            "POST",
            "/v1/uploads/complete",
            br#"{"uploadId":"missing","parts":[{"partNumber":1,"etag":"etag"}]}"#,
            false,
        )
        .await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
}

#[tokio::test]
async fn upload_transfer_rejects_control_bearing_capabilities() {
    let fixture = test_seams::fixture(&["object:write"]);
    assert_error(
        send(&fixture, "PUT", "/transfers/uploads/%00", b"payload", false).await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
}

#[tokio::test]
async fn upload_transfer_rejects_incomplete_bodies_before_repository_completion() {
    let fixture = test_seams::fixture(&["object:write"]);
    let json = reserve_upload(&fixture).await;
    let upload_url = json["data"]["uploadUrl"].as_str().expect("upload URL");
    let path = upload_url
        .strip_prefix("http://127.0.0.1:8787")
        .expect("transfer path");
    assert_error(
        send(&fixture, "PUT", path, b"", false).await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
}

#[tokio::test]
async fn upload_abort_preserves_storage_failures_and_rejects_replays() {
    let fixture = test_seams::fixture(&["object:write"]);
    let reservation = reserve_upload(&fixture).await;
    let upload_id = reservation["data"]["uploadId"].as_str().expect("upload ID");
    fixture.block_storage_delete(upload_id);
    let body =
        serde_json::to_vec(&serde_json::json!({ "uploadId": upload_id })).expect("abort JSON");
    assert_error(
        send(&fixture, "POST", "/v1/uploads/abort", &body, false).await,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    )
    .await;

    let replay_fixture = test_seams::fixture(&["object:write"]);
    let reservation = reserve_upload(&replay_fixture).await;
    let upload_id = reservation["data"]["uploadId"].as_str().expect("upload ID");
    let body =
        serde_json::to_vec(&serde_json::json!({ "uploadId": upload_id })).expect("abort JSON");
    let first = send(&replay_fixture, "POST", "/v1/uploads/abort", &body, false).await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_error(
        send(&replay_fixture, "POST", "/v1/uploads/abort", &body, false).await,
        StatusCode::CONFLICT,
        "CONFLICT",
    )
    .await;
}

#[tokio::test]
async fn transfer_handlers_conceal_foreign_reservations_without_mutation() {
    let fixture = test_seams::fixture(&["object:read", "object:write"]);
    let upload_id = fixture.seed_foreign_upload();
    let complete = serde_json::to_vec(&serde_json::json!({
        "uploadId": upload_id,
        "parts": []
    }))
    .expect("complete JSON");
    let abort =
        serde_json::to_vec(&serde_json::json!({ "uploadId": upload_id })).expect("abort JSON");
    for (method, path, body) in [
        (
            "POST",
            "/v1/uploads/parts/request".to_owned(),
            serde_json::to_vec(&serde_json::json!({
                "uploadId": upload_id,
                "partNumbers": [1]
            }))
            .expect("parts JSON"),
        ),
        ("POST", "/v1/uploads/complete".to_owned(), complete),
        ("POST", "/v1/uploads/abort".to_owned(), abort),
        (
            "GET",
            format!("/v1/uploads/status?uploadId={upload_id}"),
            Vec::new(),
        ),
    ] {
        assert_error(
            send(&fixture, method, &path, &body, false).await,
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
        )
        .await;
        assert!(fixture.is_requested(&upload_id));
    }
}

#[tokio::test]
async fn transfer_generation_failures_are_internal_and_precede_mutation() {
    let fixture = test_seams::fixture(&["object:write"]);
    for failure in [
        test_seams::IssueFailure::Clock,
        test_seams::IssueFailure::ExpiryOverflow,
        test_seams::IssueFailure::TransferUrl,
    ] {
        assert_error(
            fixture.issue_failure(failure),
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
        )
        .await;
    }
    assert_error(
        test_seams::expiry_format_failure(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    )
    .await;
    assert_error(
        fixture.put_clock_failure().await,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    )
    .await;
}
