#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{HostedMigrationOptions, checksum};
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{Path, State},
    http::{HeaderMap, Response, StatusCode, header::CONTENT_LENGTH},
    routing::{get, post},
};
use blobyard_core::SecretString;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::future::IntoFuture;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type ArtifactMap = Arc<Mutex<Option<BTreeMap<u32, Vec<u8>>>>>;

#[derive(Clone)]
pub(super) struct Fixture {
    pub(super) origin: String,
    pub(super) request: Arc<Mutex<Value>>,
    pub(super) export: Arc<Mutex<Value>>,
    pub(super) download: Arc<Mutex<Value>>,
    grant: Arc<Mutex<Value>>,
    payload: Arc<Mutex<Payload>>,
    artifacts: ArtifactMap,
}

#[derive(Clone)]
struct Payload {
    status: StatusCode,
    bytes: Vec<u8>,
    content_length: Option<u64>,
    chunked: bool,
    stream_error: bool,
}

impl Fixture {
    pub(super) async fn spawn() -> (Self, tokio::task::JoinHandle<Result<(), std::io::Error>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let origin = format!("http://{}", listener.local_addr().expect("address"));
        let fixture = Self {
            origin: origin.clone(),
            request: cell(json!({ "exportId": "export-fixture", "status": "queued" })),
            export: cell(ready_export()),
            grant: cell(json!({ "downloadUrl": format!("{origin}/payload") })),
            download: cell(download_grant(&origin, 3, &checksum(b"abc"))),
            payload: Arc::new(Mutex::new(Payload {
                status: StatusCode::OK,
                bytes: b"abc".to_vec(),
                content_length: None,
                chunked: false,
                stream_error: false,
            })),
            artifacts: Arc::new(Mutex::new(None)),
        };
        let router = Router::new()
            .route(
                "/v1/account/exports",
                post(export_request).get(export_state),
            )
            .route("/v1/account/exports/download", post(export_grant))
            .route("/v1/downloads/request", post(object_grant))
            .route("/artifacts/{part}", get(artifact))
            .route("/payload", get(payload))
            .with_state(fixture.clone());
        let server = tokio::spawn(axum::serve(listener, router).into_future());
        (fixture, server)
    }

    pub(super) fn set_payload(
        &self,
        status: StatusCode,
        bytes: &[u8],
        length: Option<u64>,
        chunked: bool,
    ) {
        *self.payload.lock().expect("payload") = Payload {
            status,
            bytes: bytes.to_vec(),
            content_length: length,
            chunked,
            stream_error: false,
        };
    }

    pub(super) fn payload_url(&self) -> SecretString {
        SecretString::new(format!("{}/payload", self.origin)).expect("payload URL")
    }

    pub(super) fn set_stream_error(&self, bytes: &[u8], length: Option<u64>) {
        *self.payload.lock().expect("payload") = Payload {
            status: StatusCode::OK,
            bytes: bytes.to_vec(),
            content_length: length,
            chunked: true,
            stream_error: true,
        };
    }

    pub(super) fn configure_complete_export(&self) {
        let mut artifacts = BTreeMap::new();
        let mut parts = Vec::new();
        for (index, dataset) in [
            "workspace",
            "projects",
            "objects",
            "versions",
            "shares",
            "retention_policies",
        ]
        .into_iter()
        .enumerate()
        {
            let part_number = u32::try_from(index + 1).expect("part number");
            let bytes = serde_json::to_vec(&json!({ "dataset": dataset, "records": [] }))
                .expect("dataset JSON");
            parts.push(json!({
                "byteSize": bytes.len(),
                "checksumSha256": checksum(&bytes),
                "dataset": dataset,
                "partNumber": part_number
            }));
            artifacts.insert(part_number, bytes);
        }
        let index = serde_json::to_vec(&json!({
            "dataset": "complete",
            "records": [{ "format": "Blob Yard account export v1", "parts": parts }]
        }))
        .expect("index JSON");
        artifacts.insert(0, index);
        *self.artifacts.lock().expect("artifacts") = Some(artifacts);
    }
}

pub(super) fn ready_export() -> Value {
    json!({
        "artifactCount": 7,
        "errorCode": null,
        "id": "export-fixture",
        "status": "ready"
    })
}

pub(super) fn download_grant(origin: &str, size: u64, digest: &str) -> Value {
    json!({
        "downloadUrl": format!("{origin}/payload"),
        "filename": "file.txt",
        "sizeBytes": size,
        "checksumSha256": digest,
        "expiresAt": "2026-07-19T00:00:00Z"
    })
}

pub(super) fn options(origin: String) -> HostedMigrationOptions {
    let mut options = HostedMigrationOptions::new(
        origin,
        std::path::PathBuf::from("destination"),
        "http://127.0.0.1:8787".to_owned(),
        Vec::new(),
        crate::StorageConfiguration::Filesystem,
    );
    options.poll_interval = Duration::from_millis(1);
    options.poll_limit = 1;
    options
}

pub(super) fn token() -> SecretString {
    SecretString::new("byd_pat_fixture").expect("token")
}

fn cell(value: Value) -> Arc<Mutex<Value>> {
    Arc::new(Mutex::new(value))
}

fn envelope(data: &Value) -> (HeaderMap, Json<Value>) {
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", "fixture-request".parse().expect("header"));
    (
        headers,
        Json(json!({ "ok": true, "data": data, "requestId": "fixture-request" })),
    )
}

async fn export_request(State(state): State<Fixture>) -> (HeaderMap, Json<Value>) {
    envelope(&state.request.lock().expect("request"))
}

async fn export_state(State(state): State<Fixture>) -> (HeaderMap, Json<Value>) {
    envelope(&state.export.lock().expect("export"))
}

async fn export_grant(
    State(state): State<Fixture>,
    Json(body): Json<Value>,
) -> (HeaderMap, Json<Value>) {
    let part = body["partNumber"].as_u64();
    let data = if state.artifacts.lock().expect("artifacts").is_some() {
        json!({ "downloadUrl": format!("{}/artifacts/{}", state.origin, part.unwrap_or(0)) })
    } else {
        state.grant.lock().expect("grant").clone()
    };
    envelope(&data)
}

async fn object_grant(State(state): State<Fixture>) -> (HeaderMap, Json<Value>) {
    envelope(&state.download.lock().expect("download"))
}

async fn artifact(State(state): State<Fixture>, Path(part): Path<u32>) -> Vec<u8> {
    state
        .artifacts
        .lock()
        .expect("artifacts")
        .as_ref()
        .and_then(|artifacts| artifacts.get(&part))
        .expect("artifact part")
        .clone()
}

async fn payload(State(state): State<Fixture>) -> Response<Body> {
    let payload = state.payload.lock().expect("payload").clone();
    let mut response = Response::builder().status(payload.status);
    if let Some(length) = payload.content_length {
        response = response.header(CONTENT_LENGTH, length);
    }
    let body = if payload.stream_error {
        let first = stream::once(async move { Ok(Bytes::from(payload.bytes)) });
        let failure = stream::once(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Err(std::io::Error::other("fixture stream failure"))
        });
        Body::from_stream(first.chain(failure))
    } else if payload.chunked || payload.content_length.is_some() {
        Body::from_stream(stream::iter([Ok::<Bytes, std::io::Error>(Bytes::from(
            payload.bytes,
        ))]))
    } else {
        Body::from(payload.bytes)
    };
    response.body(body).expect("response")
}
