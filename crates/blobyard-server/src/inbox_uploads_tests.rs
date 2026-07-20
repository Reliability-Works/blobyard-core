#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use crate::{
    auth::hash,
    contract_test_support::{assert_error, response_json, send},
    transfers::test_seams,
};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use tower::ServiceExt;

#[path = "inbox_uploads_tests/failure_contracts.rs"]
mod failure_contracts;
#[path = "inbox_uploads_tests/guest_security.rs"]
mod guest_security;
#[path = "inbox_uploads_tests/multipart.rs"]
mod multipart;

async fn guest_send(
    fixture: &test_seams::TransferFixture,
    method: &str,
    path: &str,
    body: &[u8],
    token: Option<&str>,
    idempotency: Option<&str>,
    bearer: bool,
) -> Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header(super::super::inbox_upload_auth::INBOX_HEADER, token);
    }
    if let Some(idempotency) = idempotency {
        request = request.header("idempotency-key", idempotency);
    }
    if bearer {
        request = request.header(header::AUTHORIZATION, "Bearer secret");
    }
    fixture
        .router()
        .oneshot(request.body(Body::from(body.to_vec())).expect("request"))
        .await
        .expect("response")
}

async fn create_inbox(fixture: &test_seams::TransferFixture, name: &str) -> (String, String) {
    let body = serde_json::to_vec(&serde_json::json!({
        "workspace": "fixture",
        "project": "project",
        "name": name,
        "expires": "1h"
    }))
    .expect("inbox request");
    let response = send(fixture, "POST", "/v1/inboxes", &body, false).await;
    assert_eq!(response.status(), StatusCode::OK);
    let value = response_json(response).await;
    let id = value["data"]["id"].as_str().expect("inbox ID").to_owned();
    let token = value["data"]["inboxUrl"]
        .as_str()
        .expect("inbox URL")
        .rsplit('/')
        .next()
        .expect("inbox token")
        .to_owned();
    (id, token)
}

fn upload_body(filename: &str, size: u64, checksum: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "workspace": "foreign",
        "project": "foreign",
        "path": "ignored/foreign.bin",
        "filename": filename,
        "sizeBytes": size,
        "checksumSha256": checksum,
        "contentType": "text/plain",
        "gitRepository": "example/private",
        "gitCommit": "0123456789abcdef",
        "gitBranch": "main"
    }))
    .expect("upload request")
}

async fn issue(
    fixture: &test_seams::TransferFixture,
    token: &str,
    idempotency: &str,
    body: &[u8],
) -> serde_json::Value {
    let response = guest_send(
        fixture,
        "POST",
        "/v1/uploads/request",
        body,
        Some(token),
        Some(idempotency),
        false,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

async fn issue_and_put_single(fixture: &test_seams::TransferFixture, token: &str) -> String {
    let body = upload_body("../../release notes?.txt", 5, &hash("hello"));
    let issued = issue(fixture, token, "single-journey", &body).await;
    let replayed = issue(fixture, token, "single-journey", &body).await;
    assert_eq!(issued["data"], replayed["data"]);
    assert_eq!(issued["data"]["strategy"], "single");
    let upload_id = issued["data"]["uploadId"]
        .as_str()
        .expect("upload ID")
        .to_owned();
    let upload_path = issued["data"]["uploadUrl"]
        .as_str()
        .expect("upload URL")
        .strip_prefix("http://127.0.0.1:8787")
        .expect("fixture transfer origin");
    assert_eq!(
        guest_send(fixture, "PUT", upload_path, b"hello", None, None, false)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    let status_path = format!("/v1/uploads/status?uploadId={upload_id}");
    let status = response_json(
        guest_send(fixture, "GET", &status_path, b"", Some(token), None, false).await,
    )
    .await;
    assert_eq!(status["data"]["state"], "uploading");
    upload_id
}

async fn complete_single(
    fixture: &test_seams::TransferFixture,
    token: &str,
    upload_id: &str,
) -> serde_json::Value {
    let complete = serde_json::to_vec(&serde_json::json!({
        "uploadId": upload_id,
        "parts": []
    }))
    .expect("complete request");
    response_json(
        guest_send(
            fixture,
            "POST",
            "/v1/uploads/complete",
            &complete,
            Some(token),
            None,
            false,
        )
        .await,
    )
    .await
}

fn assert_completed_inbox_record(fixture: &test_seams::TransferFixture, upload_id: &str) {
    let reservation = fixture
        .state
        .repository
        .upload_by_id(upload_id)
        .expect("completed reservation");
    assert_eq!(reservation.version.source, ObjectSource::Inbox);
    assert_eq!(reservation.version.object_path, "inbox/release_notes_.txt");
    assert_eq!(reservation.version.git_repository, None);
    assert_eq!(reservation.version.git_commit, None);
    assert_eq!(reservation.version.git_branch, None);
}

fn assert_inbox_upload_audit(fixture: &test_seams::TransferFixture) {
    let audit = fixture
        .state
        .repository
        .list_audit(&fixture.principal.workspace_id, None, 10)
        .expect("audit");
    let uploaded = audit
        .items
        .iter()
        .find(|event| event.action == "inbox.uploaded")
        .expect("inbox upload audit");
    assert!(
        uploaded
            .metadata
            .contains(&("source".to_owned(), AuditValue::String("inbox".to_owned())))
    );
    assert!(
        uploaded
            .metadata
            .contains(&("byteSize".to_owned(), AuditValue::Number(5)))
    );
}

#[tokio::test]
async fn inbox_guest_single_upload_is_scoped_verified_audited_and_retry_stable() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let (_inbox_id, token) = create_inbox(&fixture, "Release intake").await;
    let upload_id = issue_and_put_single(&fixture, &token).await;
    let completed = complete_single(&fixture, &token, &upload_id).await;
    assert_eq!(completed["data"]["sizeBytes"], 5);
    assert_eq!(completed["data"]["checksumSha256"], hash("hello"));
    assert_eq!(
        completed["data"]["uri"],
        "blobyard://fixture/project/inbox/release_notes_.txt?version=1"
    );
    assert_completed_inbox_record(&fixture, &upload_id);
    assert_inbox_upload_audit(&fixture);
}

#[test]
fn filenames_are_confined_to_one_safe_logical_component() {
    assert_eq!(
        sanitize_filename("../../release notes?.txt"),
        "release_notes_.txt"
    );
    assert_eq!(sanitize_filename("..."), "file");
    assert_eq!(sanitize_filename("\\windows\\artifact.zip"), "artifact.zip");
    assert_eq!(sanitize_filename("🔥 report 🔥"), "report");
    assert!(sanitize_filename(&"a".repeat(200)).len() <= 128);
}
