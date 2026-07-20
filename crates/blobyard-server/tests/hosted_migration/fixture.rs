use axum::{
    Json, Router,
    extract::{Path as RoutePath, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use blobyard_core::SecretString;
use blobyard_server::{HostedMigrationOptions, StorageConfiguration};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone)]
struct Fixture {
    origin: String,
    artifacts: Arc<BTreeMap<u32, Vec<u8>>>,
    object: Arc<Vec<u8>>,
    object_status: StatusCode,
}

struct RunningFixture {
    origin: String,
    object: Vec<u8>,
    server: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}

impl Drop for RunningFixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn spawn(workspace_slug: &str, object_status: StatusCode) -> RunningFixture {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let origin = format!("http://{}", listener.local_addr().expect("address"));
    let object = b"hosted-object-bytes".to_vec();
    let fixture = Fixture {
        origin: origin.clone(),
        artifacts: Arc::new(export_artifacts(workspace_slug)),
        object: Arc::new(object.clone()),
        object_status,
    };
    let router = Router::new()
        .route("/v1/account/exports", post(request_export).get(get_export))
        .route("/v1/account/exports/download", post(download_export))
        .route("/v1/downloads/request", post(request_download))
        .route("/artifacts/{part}", get(artifact))
        .route("/object", get(object_bytes))
        .with_state(fixture);
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    RunningFixture {
        origin,
        object,
        server,
    }
}

fn options(
    origin: String,
    destination: std::path::PathBuf,
    workspace_slug: &str,
) -> HostedMigrationOptions {
    let mut options = HostedMigrationOptions::new(
        origin,
        destination,
        "http://127.0.0.1:8787".to_owned(),
        vec![workspace_slug.to_owned()],
        StorageConfiguration::Filesystem,
    );
    options.poll_interval = std::time::Duration::from_millis(1);
    options.poll_limit = 2;
    options
}

fn source_token() -> SecretString {
    SecretString::new("byd_pat_fixture").expect("source token")
}

fn checksum(bytes: &[u8]) -> String {
    blobyard_core::hex_digest(&Sha256::digest(bytes))
}

async fn request_export() -> (HeaderMap, Json<Value>) {
    envelope(
        "request-export",
        &json!({ "exportId": "export-fixture", "status": "queued" }),
    )
}

async fn get_export(State(state): State<Fixture>) -> (HeaderMap, Json<Value>) {
    envelope(
        "get-export",
        &json!({
            "artifactCount": state.artifacts.len(),
            "errorCode": null,
            "expiresAt": 9_999_999_999_999_u64,
            "id": "export-fixture",
            "status": "ready"
        }),
    )
}

async fn download_export(
    State(state): State<Fixture>,
    Json(body): Json<Value>,
) -> (HeaderMap, Json<Value>) {
    let part = body["partNumber"].as_u64().expect("part number");
    envelope(
        "download-export",
        &json!({
            "downloadUrl": format!("{}/artifacts/{part}", state.origin),
            "expiresAt": 9_999_999_999_999_u64
        }),
    )
}

async fn request_download(State(state): State<Fixture>) -> (HeaderMap, Json<Value>) {
    envelope(
        "request-download",
        &json!({
            "downloadUrl": format!("{}/object", state.origin),
            "filename": "app.zip",
            "sizeBytes": state.object.len(),
            "checksumSha256": checksum(&state.object),
            "expiresAt": "2026-07-19T00:00:00Z"
        }),
    )
}

async fn artifact(State(state): State<Fixture>, RoutePath(part): RoutePath<u32>) -> Vec<u8> {
    state.artifacts.get(&part).expect("artifact").clone()
}

async fn object_bytes(State(state): State<Fixture>) -> Response {
    (state.object_status, state.object.as_ref().clone()).into_response()
}

fn envelope(request_id: &str, data: &Value) -> (HeaderMap, Json<Value>) {
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", request_id.parse().expect("request ID"));
    (
        headers,
        Json(json!({ "ok": true, "data": data, "requestId": request_id })),
    )
}

fn export_artifacts(workspace_slug: &str) -> BTreeMap<u32, Vec<u8>> {
    let (mut artifacts, parts) = dataset_artifacts(export_datasets(workspace_slug));
    let index = serde_json::to_vec(&json!({
        "dataset": "complete",
        "generatedAt": 100,
        "records": [{
            "format": "Blob Yard account export v1",
            "generatedAt": 100,
            "objectBytes": "Run each version record's downloadCommand while your account is active.",
            "parts": parts,
            "retentionEndsAt": 9_999_999_999_999_u64
        }]
    }))
    .expect("index JSON");
    artifacts.insert(0, index);
    artifacts
}

fn export_datasets(workspace_slug: &str) -> Vec<(&'static str, Vec<Value>)> {
    let object = b"hosted-object-bytes";
    let mut datasets = vec![
        (
            "workspace",
            vec![json!({
                "deletedAt": null,
                "name": "Source Workspace",
                "slug": workspace_slug,
                "workspaceReference": "workspace-source"
            })],
        ),
        (
            "projects",
            vec![json!({
                "deletedAt": null,
                "name": "Project",
                "projectReference": "project-source",
                "slug": "project",
                "workspaceReference": "workspace-source"
            })],
        ),
        (
            "objects",
            vec![json!({
                "deletedAt": null,
                "filename": "app.zip",
                "logicalPath": "releases/app.zip",
                "objectReference": "object-source",
                "projectReference": "project-source",
                "workspaceReference": "workspace-source"
            })],
        ),
        (
            "versions",
            vec![json!({
                "byteSize": object.len(),
                "checksumSha256": checksum(object),
                "contentType": "application/zip",
                "createdAt": 1000,
                "deletedAt": null,
                "gitBranch": "main",
                "gitCommit": "b".repeat(40),
                "gitRepository": "Reliability-Works/example",
                "objectReference": "object-source",
                "projectReference": "project-source",
                "source": "ci",
                "status": "ready",
                "uri": "blobyard://source/project/releases/app.zip?version=7",
                "version": 7,
                "versionReference": "version-source",
                "workspaceReference": "workspace-source"
            })],
        ),
    ];
    datasets.extend(policy_datasets());
    datasets
}

fn policy_datasets() -> [(&'static str, Vec<Value>); 2] {
    [
        (
            "shares",
            vec![json!({
                "consumedCount": 1,
                "createdAt": 2000,
                "expiresAt": 9_999_999_999_999_u64,
                "maximumDownloads": 3,
                "objectVersionReference": "version-source",
                "revokedAt": null,
                "shareReference": "share-source",
                "status": "active",
                "workspaceReference": "workspace-source"
            })],
        ),
        (
            "retention_policies",
            vec![json!({
                "branchGlob": "main",
                "createdAt": 2100,
                "enabled": true,
                "keepLatest": 4,
                "pathGlob": "releases/**",
                "projectReference": "project-source",
                "updatedAt": 2200
            })],
        ),
    ]
}

fn dataset_artifacts(
    datasets: Vec<(&'static str, Vec<Value>)>,
) -> (BTreeMap<u32, Vec<u8>>, Vec<Value>) {
    let mut artifacts = BTreeMap::new();
    let mut parts = Vec::new();
    for (index, (dataset, records)) in datasets.into_iter().enumerate() {
        let part_number = u32::try_from(index + 1).expect("part number");
        let bytes = serde_json::to_vec(&json!({
            "dataset": dataset,
            "generatedAt": 100,
            "records": records
        }))
        .expect("dataset JSON");
        parts.push(json!({
            "byteSize": bytes.len(),
            "checksumSha256": checksum(&bytes),
            "dataset": dataset,
            "downloadPath": format!("/account-exports/export-fixture/files/{part_number}"),
            "partNumber": part_number
        }));
        artifacts.insert(part_number, bytes);
    }
    (artifacts, parts)
}
