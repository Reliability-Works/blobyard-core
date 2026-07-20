//! Web Yard deploy lifecycle and management workflows.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, ok};
use blobyard_api_client::{ApiRequest, Endpoint, RawResponse};
use blobyard_core::ErrorCode;
use std::path::Path;

const SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn public_deploy_fixture(root: &Path, responses: Vec<RawResponse>) -> Fixture {
    Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "deploy",
            &root.to_string_lossy(),
            "--yard",
            "documentation",
            "--public",
        ],
        responses,
        Some("ci-token"),
        None,
    )
}

async fn failed_deploy_requests(
    fixture: &Fixture,
    expected_code: ErrorCode,
    request_count: usize,
) -> Vec<ApiRequest> {
    let error = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect_err("deploy failure");
    assert_eq!(error.code(), expected_code);
    let requests = fixture.transport.requests();
    assert_eq!(requests.len(), request_count);
    let failure = requests.last().expect("failure request");
    assert_eq!(failure.endpoint(), Endpoint::FailYardDeploy);
    assert_eq!(failure.body().expect("fail")["deployId"], "deploy_started");
    requests
}

#[path = "yards/deploy_completion_tests.rs"]
mod deploy_completion_tests;
#[path = "yards/deploy_confirmation_tests.rs"]
mod deploy_confirmation_tests;
#[path = "yards/deploy_failure_paths.rs"]
mod deploy_failure_paths;
#[path = "yards/deploy_tests.rs"]
mod deploy_tests;
#[path = "yards/management_failure_tests.rs"]
mod management_failure_tests;
#[path = "yards/management_tests.rs"]
mod management_tests;

fn start_response(
    name: &str,
    deploy_id: &str,
    manifest_root: &str,
) -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({
            "deployId": deploy_id,
            "deploymentUrl": format!("https://{name}-0123456789-team.blobyard.app"),
            "hostLabel": format!("{name}-123456789-team"),
            "manifestRoot": manifest_root,
            "status": "uploading",
            "url": format!("https://{name}-123456789-team.blobyard.app"),
            "yardId": format!("yard_{name}"),
            "yardName": name,
        }),
        "req_start",
    )
}

fn deployment_response(
    name: &str,
    deploy_id: &str,
    status: &str,
) -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({
            "deployId": deploy_id,
            "deploymentUrl": format!("https://{name}-0123456789-team.blobyard.app"),
            "status": status,
            "url": format!("https://{name}-123456789-team.blobyard.app")
        }),
        "req_deploy",
    )
}

fn failed_deploy_response() -> blobyard_api_client::RawResponse {
    ok(serde_json::json!({}), "req_fail")
}

fn yard(name: &str, current: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "id": format!("yard_{name}"), "name": name,
        "hostLabel": format!("{name}-x"),
        "url": format!("https://{name}-x.blobyard.app"), "status": "active",
        "currentDeployId": current, "projectId": "project_1", "workspaceId": "workspace_1"
    })
}

fn deploy(id: &str, status: &str, current: bool) -> serde_json::Value {
    serde_json::json!({
        "id": id, "status": status, "isCurrent": current,
        "deploymentUrl": "https://documentation-0123456789-team.blobyard.app",
        "clientDeployId": "client_1", "createdAt": 1,
        "finalisedAt": if status == "uploading" { None } else { Some(2) },
        "fileCount": 1, "totalBytes": 3, "spa": false, "cleanUrls": true
    })
}
