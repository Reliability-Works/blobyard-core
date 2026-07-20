#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::{
    Repository, api::AppState, contract_test_support::response_json,
    repository_fault_tests::FaultingRepository, transfers::test_seams,
};
use axum::{
    body::{Body, Bytes},
    http::{Request, StatusCode, header},
    response::Response,
};
use blobyard_api_client::StartYardDeployRequest;
use blobyard_core::Slug;
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

#[path = "yards_tests/contracts.rs"]
mod contract_tests;
#[path = "yards_tests/deploy_edges.rs"]
mod deploy_edge_tests;
#[path = "yards_tests/failures.rs"]
mod failure_tests;
#[path = "yards_tests/journey.rs"]
mod journey_tests;
#[path = "yards_tests/lifecycle_edges.rs"]
mod lifecycle_edge_tests;
#[path = "yards_tests/operation_edges.rs"]
mod operation_edge_tests;
#[path = "yards_tests/presentation_edges.rs"]
mod presentation_edge_tests;

pub(super) fn faulted_state(
    fixture: &test_seams::TransferFixture,
    failure_index: usize,
) -> AppState {
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let mut state = fixture.state.clone();
    state.repository = Arc::new(FaultingRepository::new(inner, failure_index));
    state
}

pub(super) fn request(client_deploy_id: &str) -> StartYardDeployRequest {
    StartYardDeployRequest {
        workspace: Slug::new("fixture").expect("workspace"),
        project: Slug::new("project").expect("project"),
        name: Slug::new("documentation").expect("yard"),
        client_deploy_id: client_deploy_id.to_owned(),
        spa: true,
        clean_urls: true,
        public: true,
    }
}

pub(super) async fn start(
    fixture: &test_seams::TransferFixture,
    client_deploy_id: &str,
) -> serde_json::Value {
    let body = serde_json::to_vec(&serde_json::json!({
        "workspace": "fixture",
        "project": "project",
        "name": "documentation",
        "clientDeployId": client_deploy_id,
        "spa": true,
        "cleanUrls": true,
        "public": true
    }))
    .expect("start request");
    let response = crate::contract_test_support::send(
        fixture,
        "POST",
        "/v1/yards/deploys/start",
        &body,
        false,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

pub(super) async fn upload_manifest(
    fixture: &test_seams::TransferFixture,
    root: &str,
    index: &[u8],
) {
    for (path, content_type, bytes) in [
        ("index.html", "text/html; charset=utf-8", index),
        ("404.html", "text/html; charset=utf-8", b"not found"),
        ("asset.js", "text/javascript; charset=utf-8", b"yard asset"),
        ("docs/index.html", "text/html; charset=utf-8", b"docs index"),
        ("guide.html", "text/html; charset=utf-8", b"clean guide"),
    ] {
        crate::previews::tests::upload(fixture, &format!("{root}{path}"), content_type, bytes)
            .await;
    }
}

pub(super) async fn mutate(
    fixture: &test_seams::TransferFixture,
    path: &str,
    value: serde_json::Value,
) -> serde_json::Value {
    let body = serde_json::to_vec(&value).expect("mutation request");
    let response = crate::contract_test_support::send(fixture, "POST", path, &body, false).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

pub(super) fn host(value: &serde_json::Value, field: &str) -> String {
    url::Url::parse(value["data"][field].as_str().expect("public URL"))
        .expect("parsed public URL")
        .host_str()
        .expect("public host")
        .to_owned()
}

pub(super) async fn public_request(
    fixture: &test_seams::TransferFixture,
    method: &str,
    path: &str,
    host: &str,
    range: Option<&str>,
) -> Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, format!("{host}:8787"));
    if let Some(range) = range {
        request = request.header(header::RANGE, range);
    }
    fixture
        .router()
        .oneshot(request.body(Body::empty()).expect("public request"))
        .await
        .expect("public response")
}

pub(super) async fn body(response: Response) -> Bytes {
    response
        .into_body()
        .collect()
        .await
        .expect("response body")
        .to_bytes()
}
