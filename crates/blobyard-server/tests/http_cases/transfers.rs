use crate::support::{
    AuthorizedServer, assert_upload_status, authorized_server, complete_single_upload, send,
    send_bytes, send_idempotent, send_range, transfer_path,
};
use axum::{Router, http::StatusCode};
use blobyard_server::initialize;
use serde_json::{Value, json};

const HELLO_CHECKSUM: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

#[tokio::test]
async fn upload_is_retry_stable_streamed_once_and_durable_across_restart() {
    let server = authorized_server().await;
    create_project(&server).await;
    let request = upload_request("builds/hello.txt", HELLO_CHECKSUM);
    let first = reserve(&server, request.clone(), "stable-upload").await;
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(first.1["data"]["strategy"], "single");
    let (router, replay) = replay_after_restart(&server, request).await;
    assert_stable_reservation(&first.1, &replay);
    upload_once(&router, &upload_path(&replay)).await;
    let upload_id = first.1["data"]["uploadId"].as_str().expect("upload ID");
    complete_upload(&router, &server.access_token, upload_id).await;
}

async fn replay_after_restart(server: &AuthorizedServer, request: Value) -> (Router, Value) {
    let mut reopened = initialize(server.temporary.path()).expect("reopened server");
    assert!(reopened.take_bootstrap_token().is_none());
    let router = reopened.router();
    let replay = send_idempotent(
        &router,
        "POST",
        "/v1/uploads/request",
        Some(request),
        Some(&server.access_token),
        Some("stable-upload"),
    )
    .await;
    assert_eq!(replay.0, StatusCode::OK);
    (router, replay.1)
}

fn assert_stable_reservation(first: &Value, replay: &Value) {
    assert_eq!(replay["data"]["uploadId"], first["data"]["uploadId"]);
    assert_eq!(replay["data"]["uploadUrl"], first["data"]["uploadUrl"]);
}

async fn upload_once(router: &Router, upload_path: &str) {
    let upload = send_bytes(
        router,
        "PUT",
        upload_path,
        b"hello".to_vec(),
        None,
        None,
        Some("text/plain"),
    )
    .await;
    assert_eq!(upload.0, StatusCode::NO_CONTENT);
    assert!(upload.1.is_empty());

    let consumed = send_bytes(
        router,
        "PUT",
        upload_path,
        b"hello".to_vec(),
        None,
        None,
        Some("text/plain"),
    )
    .await;
    assert_eq!(consumed.0, StatusCode::NOT_FOUND);
    let consumed_error: Value = serde_json::from_slice(&consumed.1).expect("error JSON");
    assert_eq!(consumed_error["error"]["code"], "NOT_FOUND");
}

async fn complete_upload(router: &Router, token: &str, upload_id: &str) {
    assert_upload_status(router, token, upload_id, "uploading").await;
    let complete = complete_single_upload(router, token, upload_id).await;
    assert_eq!(complete.0, StatusCode::OK);
    assert_eq!(complete.1["data"]["sizeBytes"], 5);
    assert_eq!(complete.1["data"]["checksumSha256"], HELLO_CHECKSUM);
    assert_eq!(
        complete.1["data"]["uri"],
        "blobyard://default/documentation/builds/hello.txt?version=1"
    );
    assert_upload_status(router, token, upload_id, "complete").await;
}

#[tokio::test]
async fn upload_rejects_idempotency_drift_integrity_mismatch_and_aborted_grants() {
    let server = authorized_server().await;
    create_project(&server).await;
    let first = assert_idempotency_drift(&server).await;
    assert_integrity_retry(&server.router, &upload_path(&first)).await;
    assert_aborted_grant(&server).await;
}

#[tokio::test]
async fn completed_objects_list_and_stream_full_and_ranged_downloads() {
    let server = authorized_server().await;
    create_project(&server).await;
    let uri = store_hello(&server).await;

    let list = send(
        &server.router,
        "GET",
        "/v1/objects?workspace=default&project=documentation&versions=false",
        None,
        Some(&server.access_token),
    )
    .await;
    assert_eq!(list.0, StatusCode::OK);
    let item = &list.1["data"]["items"][0];
    assert_eq!(item["uri"], uri);
    assert_eq!(item["availability"], "available");
    assert_eq!(item["source"], "cli");
    assert!(
        item["createdAt"]
            .as_str()
            .is_some_and(|value| value.ends_with('Z'))
    );

    let grant = send(
        &server.router,
        "POST",
        "/v1/downloads/request",
        Some(json!({ "uri": uri })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(grant.0, StatusCode::OK);
    assert_eq!(grant.1["data"]["sizeBytes"], 5);
    assert_eq!(grant.1["data"]["checksumSha256"], HELLO_CHECKSUM);
    let path = transfer_path(&grant.1, "downloadUrl");

    let full = send_range(&server.router, &path, None).await;
    assert_eq!(full.0, StatusCode::OK);
    assert_eq!(full.2, b"hello");
    assert_eq!(full.1["accept-ranges"], "bytes");

    let partial = send_range(&server.router, &path, Some("bytes=1-3")).await;
    assert_eq!(partial.0, StatusCode::PARTIAL_CONTENT);
    assert_eq!(partial.2, b"ell");
    assert_eq!(partial.1["content-range"], "bytes 1-3/5");
    assert_eq!(partial.1["content-length"], "3");

    let suffix = send_range(&server.router, &path, Some("bytes=-2")).await;
    assert_eq!(suffix.0, StatusCode::PARTIAL_CONTENT);
    assert_eq!(suffix.2, b"lo");
    let invalid = send_range(&server.router, &path, Some("bytes=9-10")).await;
    assert_eq!(invalid.0, StatusCode::RANGE_NOT_SATISFIABLE);
}

async fn store_hello(server: &AuthorizedServer) -> String {
    let reservation = reserve(
        server,
        upload_request("builds/download.txt", HELLO_CHECKSUM),
        "download-fixture",
    )
    .await;
    let uploaded = send_bytes(
        &server.router,
        "PUT",
        &upload_path(&reservation.1),
        b"hello".to_vec(),
        None,
        None,
        Some("text/plain"),
    )
    .await;
    assert_eq!(uploaded.0, StatusCode::NO_CONTENT);
    let upload_id = reservation.1["data"]["uploadId"]
        .as_str()
        .expect("upload ID");
    let complete = complete_single_upload(&server.router, &server.access_token, upload_id).await;
    assert_eq!(complete.0, StatusCode::OK);
    complete.1["data"]["uri"]
        .as_str()
        .expect("object URI")
        .to_owned()
}

async fn assert_idempotency_drift(server: &AuthorizedServer) -> Value {
    let first = reserve(
        server,
        upload_request("builds/first.txt", HELLO_CHECKSUM),
        "fixed-key",
    )
    .await;
    assert_eq!(first.0, StatusCode::OK);
    let drift = reserve(
        server,
        upload_request("builds/other.txt", HELLO_CHECKSUM),
        "fixed-key",
    )
    .await;
    assert_eq!(drift.0, StatusCode::CONFLICT);
    first.1
}

async fn assert_integrity_retry(router: &Router, upload_path: &str) {
    let mismatch = send_bytes(
        router,
        "PUT",
        upload_path,
        b"HELLO".to_vec(),
        None,
        None,
        Some("text/plain"),
    )
    .await;
    assert_eq!(mismatch.0, StatusCode::BAD_REQUEST);
    let retry = send_bytes(
        router,
        "PUT",
        upload_path,
        b"hello".to_vec(),
        None,
        None,
        Some("text/plain"),
    )
    .await;
    assert_eq!(retry.0, StatusCode::NO_CONTENT);
}

async fn assert_aborted_grant(server: &AuthorizedServer) {
    let aborted = reserve(
        server,
        upload_request("builds/aborted.txt", HELLO_CHECKSUM),
        "aborted-key",
    )
    .await;
    let aborted_id = aborted.1["data"]["uploadId"].as_str().expect("upload ID");
    let abort = send(
        &server.router,
        "POST",
        "/v1/uploads/abort",
        Some(json!({ "uploadId": aborted_id })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(abort.0, StatusCode::OK);
    assert_upload_status(&server.router, &server.access_token, aborted_id, "aborted").await;
    let rejected = send_bytes(
        &server.router,
        "PUT",
        &upload_path(&aborted.1),
        b"hello".to_vec(),
        None,
        None,
        Some("text/plain"),
    )
    .await;
    assert_eq!(rejected.0, StatusCode::NOT_FOUND);
}

async fn create_project(server: &AuthorizedServer) {
    let project = send(
        &server.router,
        "POST",
        "/v1/projects",
        Some(json!({ "workspace": "default", "name": "Documentation" })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(project.0, StatusCode::OK);
}

fn upload_request(path: &str, checksum: &str) -> Value {
    json!({
        "workspace": "default",
        "project": "documentation",
        "path": path,
        "filename": "hello.txt",
        "sizeBytes": 5,
        "checksumSha256": checksum,
        "contentType": "text/plain"
    })
}

async fn reserve(
    server: &AuthorizedServer,
    request: Value,
    idempotency: &str,
) -> (StatusCode, Value) {
    send_idempotent(
        &server.router,
        "POST",
        "/v1/uploads/request",
        Some(request),
        Some(&server.access_token),
        Some(idempotency),
    )
    .await
}

fn upload_path(response: &Value) -> String {
    transfer_path(response, "uploadUrl")
}
