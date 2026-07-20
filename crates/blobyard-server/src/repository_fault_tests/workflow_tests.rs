#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::FaultingRepository;
use crate::{Repository, api, auth};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use blobyard_contract::{CredentialRepository, MetadataRepository, ObjectStorage, WorkspaceRecord};
use blobyard_core::{SecretString, Slug};
use blobyard_repository_sqlite::SqliteRepository;
use blobyard_storage_filesystem::FilesystemStorage;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

#[path = "aborted_workflow_tests.rs"]
mod aborted_workflow;
#[path = "bootstrap_workflow_tests.rs"]
mod bootstrap_workflow;
#[path = "failure_assertion_tests.rs"]
mod failure_assertion;

struct Fixture {
    _temporary: tempfile::TempDir,
    router: Router,
}

fn fixture(failure_index: usize) -> Fixture {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository = Arc::new(
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository"),
    );
    let workspace = WorkspaceRecord {
        id: "workspace_default".to_owned(),
        name: "Default".to_owned(),
        slug: Slug::new("default").expect("slug"),
    };
    repository.create_workspace(&workspace).expect("workspace");
    repository
        .install_bootstrap(&auth::hash("bootstrap"))
        .expect("bootstrap");
    let inner: Arc<dyn Repository> = repository;
    let faulting: Arc<dyn Repository> = Arc::new(FaultingRepository::new(inner, failure_index));
    let storage: Arc<dyn ObjectStorage> =
        Arc::new(FilesystemStorage::open(&temporary.path().join("objects")).expect("storage"));
    let staging = temporary.path().join("staging");
    std::fs::create_dir(&staging).expect("staging");
    Fixture {
        router: api::router(
            faulting,
            storage,
            workspace,
            Arc::new(SecretString::new("capability").expect("capability")),
            "http://127.0.0.1:8787".to_owned(),
            "http://localhost:8787".to_owned(),
            staging,
        ),
        _temporary: temporary,
    }
}

async fn send(
    router: &Router,
    method: &str,
    path: &str,
    body: Vec<u8>,
    bearer: Option<&str>,
    idempotency: Option<&str>,
) -> (StatusCode, Vec<u8>) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = bearer {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(value) = idempotency {
        request = request.header("idempotency-key", value);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::from(body)).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes()
        .to_vec();
    (status, bytes)
}

async fn json_request(
    router: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    idempotency: Option<&str>,
    token: &str,
) -> Result<Value, (String, StatusCode)> {
    let bytes = body.map_or_else(Vec::new, |value| serde_json::to_vec(&value).expect("JSON"));
    let (status, response) = send(router, method, path, bytes, Some(token), idempotency).await;
    if status.is_success() {
        Ok(serde_json::from_slice(&response).expect("response JSON"))
    } else {
        Err((path.to_owned(), status))
    }
}

fn transfer_path(response: &Value, field: &str) -> String {
    let value = response["data"][field].as_str().expect("transfer URL");
    url::Url::parse(value).expect("URL").path().to_owned()
}

async fn transfer(
    router: &Router,
    method: &str,
    path: &str,
) -> Result<Vec<u8>, (String, StatusCode)> {
    let (status, body) = send(router, method, path, Vec::new(), None, None).await;
    if status.is_success() {
        Ok(body)
    } else {
        Err((path.to_owned(), status))
    }
}

async fn run_workflow(router: &Router) -> Result<(), (String, StatusCode)> {
    let token = bootstrap_workflow::run(router).await?;
    run_namespace(router, &token).await?;
    let (uri, download_path) = run_object_transfer(router, &token).await?;
    transfer(router, "GET", &download_path).await?;
    run_lifecycle(router, uri, &token).await
}

async fn run_namespace(router: &Router, token: &str) -> Result<(), (String, StatusCode)> {
    json_request(router, "GET", "/v1/workspaces", None, None, token).await?;
    json_request(
        router,
        "POST",
        "/v1/workspaces",
        Some(json!({ "name": "Other" })),
        None,
        token,
    )
    .await?;
    json_request(
        router,
        "POST",
        "/v1/projects",
        Some(json!({ "workspace": "default", "name": "Fixture" })),
        None,
        token,
    )
    .await?;
    json_request(
        router,
        "GET",
        "/v1/projects?workspace=default",
        None,
        None,
        token,
    )
    .await?;
    Ok(())
}

async fn run_object_transfer(
    router: &Router,
    token: &str,
) -> Result<(Value, String), (String, StatusCode)> {
    let upload = json_request(
        router,
        "POST",
        "/v1/uploads/request",
        Some(json!({
            "workspace": "default", "project": "fixture", "path": "fixture.txt",
            "filename": "fixture.txt", "sizeBytes": 0,
            "checksumSha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "contentType": "text/plain"
        })),
        Some("fixture-upload"),
        token,
    )
    .await?;
    let upload_id = upload["data"]["uploadId"].as_str().expect("upload ID");
    upload_status(router, upload_id, token).await?;
    transfer(router, "PUT", &transfer_path(&upload, "uploadUrl")).await?;
    upload_status(router, upload_id, token).await?;
    let completed = json_request(
        router,
        "POST",
        "/v1/uploads/complete",
        Some(json!({ "uploadId": upload_id, "parts": [] })),
        None,
        token,
    )
    .await?;
    upload_status(router, upload_id, token).await?;
    aborted_workflow::run(router, token).await?;
    json_request(
        router,
        "GET",
        "/v1/objects?workspace=default&project=fixture&versions=false",
        None,
        None,
        token,
    )
    .await?;
    let uri = completed["data"]["uri"].clone();
    let download = json_request(
        router,
        "POST",
        "/v1/downloads/request",
        Some(json!({ "uri": uri })),
        None,
        token,
    )
    .await?;
    Ok((uri, transfer_path(&download, "downloadUrl")))
}

async fn upload_status(
    router: &Router,
    upload_id: &str,
    token: &str,
) -> Result<Value, (String, StatusCode)> {
    json_request(
        router,
        "GET",
        &format!("/v1/uploads/status?uploadId={upload_id}"),
        None,
        None,
        token,
    )
    .await
}

async fn run_lifecycle(
    router: &Router,
    uri: Value,
    token: &str,
) -> Result<(), (String, StatusCode)> {
    json_request(
        router,
        "PUT",
        "/v1/retention",
        Some(json!({ "workspace": "default", "project": "fixture", "keepLatest": 1 })),
        None,
        token,
    )
    .await?;
    for path in [
        "/v1/retention?workspace=default&project=fixture",
        "/v1/retention/overview?workspace=default&project=fixture",
        "/v1/audit?workspace=default",
    ] {
        json_request(router, "GET", path, None, None, token).await?;
    }
    json_request(
        router,
        "DELETE",
        "/v1/retention?workspace=default&project=fixture",
        None,
        None,
        token,
    )
    .await?;
    json_request(
        router,
        "DELETE",
        "/v1/objects",
        Some(json!({ "uri": uri })),
        None,
        token,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn handlers_map_each_repository_failure_and_preserve_the_full_success_path() {
    let mut completed = false;
    for failure_index in 0..128 {
        let fixture = fixture(failure_index);
        match run_workflow(&fixture.router).await {
            Ok(()) => {
                assert!(failure_index > 0);
                completed = true;
                break;
            }
            Err((path, status)) => failure_assertion::assert(&path, status, failure_index),
        }
    }
    assert!(completed, "repository failure sweep exceeded its bound");
}
