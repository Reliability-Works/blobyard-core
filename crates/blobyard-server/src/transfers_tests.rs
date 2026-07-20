#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::Repository;
use crate::api::AppState;
use crate::auth::{Principal, hash};
use crate::error::ApiError;
use crate::inbox_upload_auth::UploadAuthority;
use crate::repository_fault_tests::{Corruption, FaultingRepository};
use axum::{
    Json, Router,
    body::Body,
    http::{Request, StatusCode, header},
    response::{IntoResponse, Response},
};
use blobyard_api_client::{AbortUploadRequest, RequestUploadRequest, UploadStatusQuery};
use blobyard_contract::{LocalApiTokenRecord, ProjectRecord};
use blobyard_core::{SecretString, Slug};
use http_body_util::BodyExt;
use rusqlite::Connection;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tempfile::TempDir;
use tower::ServiceExt;

#[path = "transfer_tests/delete_counting_storage_tests.rs"]
mod delete_counting_storage;

#[path = "transfer_tests/multipart_failure_tests.rs"]
mod multipart_failure_tests;

fn slug(value: &str) -> Slug {
    Slug::new(value.to_owned()).expect("slug")
}

fn request(path: &str) -> RequestUploadRequest {
    RequestUploadRequest {
        workspace: slug("fixture"),
        project: slug("project"),
        path: path.to_owned(),
        filename: "fixture.bin".to_owned(),
        size_bytes: 4,
        checksum_sha256: "00".repeat(32),
        content_type: "application/octet-stream".to_owned(),
        git_repository: None,
        git_commit: None,
        git_branch: None,
    }
}

fn fixture() -> (TempDir, AppState, ProjectRecord) {
    let root = TempDir::new().expect("root");
    let staging = root.path().join("staging");
    std::fs::create_dir(&staging).expect("staging");
    let state = crate::test_support::filesystem_state(&root, staging);
    state
        .repository
        .create_workspace(&state.default_workspace)
        .expect("workspace");
    let project = ProjectRecord {
        id: "project_fixture".to_owned(),
        workspace_id: state.default_workspace.id.clone(),
        slug: slug("project"),
        name: "Project".to_owned(),
    };
    state.repository.create_project(&project).expect("project");
    let token = LocalApiTokenRecord {
        id: "token_fixture".to_owned(),
        name: "Fixture".to_owned(),
        token_prefix: "bya_fixture".to_owned(),
        secret_hash: hash("secret"),
        scopes: vec!["object:read".to_owned(), "object:write".to_owned()],
        workspace_id: state.default_workspace.id.clone(),
        project_id: None,
        created_at_ms: 1,
        expires_at_ms: i64::MAX as u64,
        last_used_at_ms: None,
        revoked_at_ms: None,
    };
    state
        .repository
        .install_bootstrap(&hash("bootstrap"))
        .expect("bootstrap");
    state
        .repository
        .exchange_bootstrap(
            &hash("bootstrap"),
            &token,
            &blobyard_testkit::cli_session_record(&token, env!("CARGO_PKG_VERSION")),
        )
        .expect("access token");
    (root, state, project)
}

#[test]
fn transfer_request_propagates_clock_failure_before_authorization() {
    let fixture = crate::transfers::test_seams::fixture(&["object:write"]);
    let result = super::transfer_request(
        &fixture.state,
        UploadAuthority::Operator(Principal(fixture.principal)),
        Ok(Json(AbortUploadRequest {
            upload_id: "missing".to_owned(),
        })),
        Err(ApiError::internal()),
    );
    assert_eq!(
        result
            .err()
            .expect("clock failure")
            .into_response()
            .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn upload_issue_and_status_propagate_clock_failure_before_authorization() {
    let fixture = crate::transfers::test_seams::fixture(&["object:write"]);
    let authority = || UploadAuthority::Operator(Principal(fixture.principal.clone()));
    let issue = super::request_upload_at(
        &fixture.state,
        authority(),
        &axum::http::HeaderMap::new(),
        Ok(Json(request("clock/failure"))),
        Err(ApiError::internal()),
    );
    assert_eq!(
        issue
            .err()
            .expect("issue clock failure")
            .into_response()
            .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let status = super::upload_status_at(
        &fixture.state,
        authority(),
        &UploadStatusQuery {
            upload_id: "missing".to_owned(),
        },
        Err(ApiError::internal()),
    );
    assert_eq!(
        status
            .err()
            .expect("status clock failure")
            .into_response()
            .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

fn reserve_uploaded(state: &AppState, project: &ProjectRecord, upload_id: &str) {
    let capability = SecretString::new("capability").expect("capability");
    let input = crate::transfer_grants::reservation_input(
        &request("valid/path"),
        project,
        upload_id,
        &capability,
        u64::try_from(i64::MAX).expect("maximum expiry"),
        blobyard_contract::ObjectSource::Cli,
    );
    state
        .repository
        .reserve_upload(&input)
        .expect("reservation");
    state
        .repository
        .record_uploaded_bytes(upload_id, 4, &"00".repeat(32))
        .expect("uploaded bytes");
}

fn router(state: &AppState) -> Router {
    crate::transfers::test_seams::fixture_router(state)
}

async fn send_json(
    state: &AppState,
    method: &str,
    path: &str,
    body: serde_json::Value,
    idempotency: Option<&str>,
) -> Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, "Bearer secret")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(value) = idempotency {
        request = request.header("idempotency-key", value);
    }
    router(state)
        .oneshot(
            request
                .body(Body::from(serde_json::to_vec(&body).expect("request JSON")))
                .expect("request"),
        )
        .await
        .expect("response")
}

async fn assert_error(response: Response, status: StatusCode, code: &str, message: &str) {
    assert_eq!(response.status(), status);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("error body")
        .to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).expect("error JSON");
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], code);
    assert_eq!(value["error"]["message"], message);
    assert!(
        value["requestId"]
            .as_str()
            .is_some_and(|request_id| request_id.starts_with("req_"))
    );
}

async fn assert_internal(response: Response) {
    assert_error(
        response,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
        "Blobyard couldn't complete that. Try again or contact support.",
    )
    .await;
}

#[tokio::test]
async fn upload_request_rejects_an_invalid_object_path_after_project_resolution() {
    let (_root, state, _project) = fixture();
    let response = send_json(
        &state,
        "POST",
        "/v1/uploads/request",
        serde_json::to_value(request("/absolute")).expect("request value"),
        Some("fixture"),
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

#[tokio::test]
async fn upload_completion_conceals_corrupt_persisted_object_paths() {
    let (root, state, project) = fixture();
    let capability = SecretString::new("capability").expect("capability");
    let input = crate::transfer_grants::reservation_input(
        &request("valid/path"),
        &project,
        "upload_fixture",
        &capability,
        u64::try_from(i64::MAX).expect("maximum expiry"),
        blobyard_contract::ObjectSource::Cli,
    );
    let reservation = state
        .repository
        .reserve_upload(&input)
        .expect("reservation");
    state
        .repository
        .record_uploaded_bytes("upload_fixture", 4, &"00".repeat(32))
        .expect("uploaded bytes");
    Connection::open(root.path().join("metadata.sqlite3"))
        .expect("connection")
        .execute(
            "UPDATE object_versions SET object_path = '/absolute' WHERE id = ?1",
            [&reservation.version.id],
        )
        .expect("corrupt path");

    let response = send_json(
        &state,
        "POST",
        "/v1/uploads/complete",
        serde_json::json!({ "uploadId": "upload_fixture", "parts": [] }),
        None,
    )
    .await;
    assert_internal(response).await;
}

#[tokio::test]
async fn upload_completion_conceals_every_corrupt_repository_result() {
    for corruption in [
        Corruption::CompletedVersion,
        Corruption::CompletedPath,
        Corruption::CompletedSize,
        Corruption::CompletedChecksum,
    ] {
        let (_root, mut state, project) = fixture();
        reserve_uploaded(&state, &project, "upload_corrupt");
        let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
        state.repository = Arc::new(FaultingRepository::corrupting(inner, corruption));
        let response = send_json(
            &state,
            "POST",
            "/v1/uploads/complete",
            serde_json::json!({ "uploadId": "upload_corrupt", "parts": [] }),
            None,
        )
        .await;
        assert_internal(response).await;
    }
}

#[path = "transfers_access_tests.rs"]
mod access;

#[path = "transfer_multipart_tests.rs"]
mod multipart;
