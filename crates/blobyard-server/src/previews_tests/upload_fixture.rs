use crate::{contract_test_support::response_json, transfers::test_seams};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

/// Uploads one fixture object through the real reservation and completion routes.
pub async fn upload(
    fixture: &test_seams::TransferFixture,
    path: &str,
    content_type: &str,
    bytes: &[u8],
) {
    let (body, idempotency_key) = upload_request(path, content_type, bytes);
    let reserved = reserve_upload(fixture, path, body, idempotency_key).await;
    write_upload(fixture, &reserved, bytes).await;
    complete_upload(fixture, &reserved).await;
}

fn upload_request(path: &str, content_type: &str, bytes: &[u8]) -> (Vec<u8>, String) {
    let checksum = blobyard_core::hex_digest(&Sha256::digest(bytes));
    let mut idempotency_digest = Sha256::new();
    idempotency_digest.update(path.as_bytes());
    idempotency_digest.update([0]);
    idempotency_digest.update(bytes);
    let idempotency_key = format!(
        "manifest-{}",
        blobyard_core::hex_digest(&idempotency_digest.finalize())
    );
    let body = serde_json::to_vec(&serde_json::json!({
        "workspace": "fixture",
        "project": "project",
        "path": path,
        "filename": path.rsplit('/').next().expect("filename"),
        "sizeBytes": bytes.len(),
        "checksumSha256": checksum,
        "contentType": content_type
    }))
    .expect("upload request");
    (body, idempotency_key)
}

async fn reserve_upload(
    fixture: &test_seams::TransferFixture,
    path: &str,
    body: Vec<u8>,
    idempotency_key: String,
) -> serde_json::Value {
    let reserved = response_json(
        fixture
            .router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/uploads/request")
                    .header(header::AUTHORIZATION, "Bearer secret")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", idempotency_key)
                    .body(Body::from(body))
                    .expect("upload reservation request"),
            )
            .await
            .expect("upload reservation response"),
    )
    .await;
    assert_eq!(
        reserved["ok"], true,
        "upload reservation for {path}: {reserved}"
    );
    reserved
}

async fn write_upload(
    fixture: &test_seams::TransferFixture,
    reserved: &serde_json::Value,
    bytes: &[u8],
) {
    let upload_path = reserved["data"]["uploadUrl"]
        .as_str()
        .expect("upload URL")
        .strip_prefix("http://127.0.0.1:8787")
        .expect("upload path");
    assert_eq!(
        crate::contract_test_support::send(fixture, "PUT", upload_path, bytes, false)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
}

async fn complete_upload(fixture: &test_seams::TransferFixture, reserved: &serde_json::Value) {
    let complete = serde_json::to_vec(&serde_json::json!({
        "uploadId": reserved["data"]["uploadId"],
        "parts": []
    }))
    .expect("complete request");
    assert_eq!(
        crate::contract_test_support::send(
            fixture,
            "POST",
            "/v1/uploads/complete",
            &complete,
            false,
        )
        .await
        .status(),
        StatusCode::OK
    );
}
