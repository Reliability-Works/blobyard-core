#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::contract_test_support::{assert_error, send};
use axum::http::StatusCode;
use blobyard_server::{objects::test_seams, transfers::test_seams as transfer_seams};
use http_body_util::BodyExt;

#[tokio::test]
async fn object_routes_reject_missing_scopes_before_durable_mutation() {
    let fixture = transfer_seams::fixture(&["fixture"]);
    let counts = fixture.object_audit_counts();
    for (method, path, body) in [
        (
            "DELETE",
            "/v1/objects",
            br#"{"uri":"blobyard://fixture/project/object.bin"}"#.as_slice(),
        ),
        (
            "GET",
            "/v1/objects?workspace=fixture&project=project&versions=false",
            b"".as_slice(),
        ),
        (
            "POST",
            "/v1/downloads/request",
            br#"{"uri":"blobyard://fixture/project/object.bin"}"#.as_slice(),
        ),
    ] {
        assert_error(
            send(&fixture, method, path, body, false).await,
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        )
        .await;
        assert_eq!(fixture.object_audit_counts(), counts);
    }
}

#[tokio::test]
async fn object_routes_reject_malformed_inputs_before_durable_mutation() {
    let fixture = transfer_seams::fixture(&["object:read", "object:write"]);
    let counts = fixture.object_audit_counts();
    for (method, path, body) in [
        ("DELETE", "/v1/objects", b"{".as_slice()),
        ("POST", "/v1/downloads/request", b"{".as_slice()),
        (
            "POST",
            "/v1/downloads/request",
            br#"{"uri":"not-a-uri"}"#.as_slice(),
        ),
        (
            "GET",
            "/v1/objects?workspace=fixture&project=project&versions=false&prefix=../",
            b"".as_slice(),
        ),
    ] {
        assert_error(
            send(&fixture, method, path, body, false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
        assert_eq!(fixture.object_audit_counts(), counts);
    }
}

#[tokio::test]
async fn request_download_validates_every_deterministic_input_before_effects() {
    for failure in [
        test_seams::RequestFailure::Clock,
        test_seams::RequestFailure::ExpiryOverflow,
        test_seams::RequestFailure::TransferUrl,
        test_seams::RequestFailure::MissingSize,
        test_seams::RequestFailure::MissingChecksum,
        test_seams::RequestFailure::ExpiryFormat,
    ] {
        let fixture = transfer_seams::fixture(&["object:read"]);
        assert_error(
            fixture.request_download_failure(failure),
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
        )
        .await;
    }

    assert_error(
        test_seams::list_repository_failure(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    )
    .await;
    let fixture = transfer_seams::fixture(&["object:read"]);
    assert_error(
        fixture.list_corrupt_record_failure(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    )
    .await;
}

#[tokio::test]
async fn valid_download_request_commits_one_grant_and_audit() {
    let fixture = transfer_seams::fixture(&["object:read"]);
    let response = fixture.request_download_success();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("response body")
        .to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).expect("response JSON");
    assert_eq!(value["ok"], true);
    assert_eq!(value["data"]["filename"], "object.bin");
    assert_eq!(value["data"]["sizeBytes"], 1);
    assert_eq!(value["data"]["checksumSha256"], "00".repeat(32));
    assert_eq!(value["data"]["expiresAt"], "1970-01-01T00:15:00Z");
}

#[tokio::test]
async fn object_deletion_and_download_lookup_fail_before_mutation_or_io() {
    let fixture = transfer_seams::fixture(&["object:read", "object:write"]);
    assert_error(
        fixture.delete_clock_failure(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    )
    .await;

    for (failure, status, code) in [
        (
            test_seams::DownloadFailure::MalformedCapability,
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
        ),
        (
            test_seams::DownloadFailure::Clock,
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
        ),
    ] {
        assert_error(fixture.download_failure(failure).await, status, code).await;
    }
}
