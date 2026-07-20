//! Complete wire encoders and decoders for Web Yard models.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use blobyard_api_client::{
    DeleteWebYardRequest, FailYardDeployRequest, ListWebYardsQuery, ListYardDeploysQuery,
    RollbackWebYardRequest, StartYardDeployRequest, StartYardDeployResponse, WebYardPage,
    YardDeployMutationRequest, YardDeployPage, YardDeploymentResponse,
};
use blobyard_core::Slug;

#[test]
fn web_yard_deploy_requests_encode_exactly() {
    let workspace = Slug::new("team").expect("workspace");
    let project = Slug::new("web").expect("project");
    let yard = Slug::new("docs").expect("yard");
    assert_eq!(
        StartYardDeployRequest {
            workspace,
            project,
            name: yard,
            client_deploy_id: "deploy_1".into(),
            spa: true,
            clean_urls: false,
            public: true,
        }
        .into_json(),
        serde_json::json!({
            "workspace": "team", "project": "web", "name": "docs",
            "clientDeployId": "deploy_1", "spa": true, "cleanUrls": false, "public": true
        })
    );
    assert_eq!(
        YardDeployMutationRequest {
            deploy_id: "deploy_1".into(),
        }
        .into_json(),
        serde_json::json!({ "deployId": "deploy_1" })
    );
    assert_eq!(
        FailYardDeployRequest {
            deploy_id: "deploy_1".into(),
            failure_code: "INTERRUPTED".into(),
            failure_message: "The operation was cancelled.".into(),
        }
        .into_json(),
        serde_json::json!({
            "deployId": "deploy_1",
            "failureCode": "INTERRUPTED", "failureMessage": "The operation was cancelled."
        })
    );
}

#[test]
fn web_yard_management_requests_encode_exactly() {
    assert_eq!(
        ListWebYardsQuery {
            workspace: Slug::new("team").expect("workspace"),
            project: Slug::new("web").expect("project"),
        }
        .into_query(),
        "workspace=team&project=web"
    );
    assert_eq!(
        ListYardDeploysQuery {
            yard_id: "yard_1".into(),
        }
        .into_query(),
        "yardId=yard_1"
    );
    assert_eq!(
        RollbackWebYardRequest {
            yard_id: "yard_1".into(),
            deploy_id: Some("deploy_1".into())
        }
        .into_json(),
        serde_json::json!({ "yardId": "yard_1", "deployId": "deploy_1" })
    );
    assert_eq!(
        DeleteWebYardRequest {
            yard_id: "yard_1".into(),
        }
        .into_json(),
        serde_json::json!({ "yardId": "yard_1" })
    );
    assert_eq!(
        RollbackWebYardRequest {
            yard_id: "yard_1".into(),
            deploy_id: None,
        }
        .into_json(),
        serde_json::json!({ "yardId": "yard_1" })
    );
}

#[test]
fn web_yard_responses_decode_the_http_contract() {
    let started: StartYardDeployResponse = serde_json::from_value(serde_json::json!({
        "deployId": "deploy_1",
        "deploymentUrl": "https://docs-0123456789-main.blobyard.app",
        "hostLabel": "docs-123456789-main",
        "manifestRoot": ".blobyard-yard/yard_1/client_1/",
        "status": "uploading",
        "url": "https://docs-123456789-main.blobyard.app",
        "yardId": "yard_1",
        "yardName": "docs"
    }))
    .expect("start response");
    assert_eq!(started.deploy_id, "deploy_1");
    assert_eq!(started.yard_name.as_str(), "docs");

    let yards: WebYardPage = serde_json::from_value(serde_json::json!({
        "items": [{
            "currentDeployId": "deploy_1", "hostLabel": "docs-123456789-main", "id": "yard_1",
            "name": "docs", "projectId": "project_1", "status": "active",
            "url": "https://docs-123456789-main.blobyard.app", "workspaceId": "workspace_1"
        }],
        "nextCursor": null
    }))
    .expect("yard page");
    assert_eq!(yards.items()[0].id, "yard_1");

    let deploys: YardDeployPage = serde_json::from_value(serde_json::json!({
        "items": [{
            "cleanUrls": true, "clientDeployId": "client_1", "createdAt": 10,
            "deploymentUrl": "https://docs-0123456789-main.blobyard.app",
            "fileCount": 2, "finalisedAt": 20, "id": "deploy_1", "isCurrent": true,
            "spa": false, "status": "live", "totalBytes": 42
        }],
        "nextCursor": null
    }))
    .expect("deploy page");
    assert!(deploys.items()[0].is_current);

    let deployment: YardDeploymentResponse = serde_json::from_value(serde_json::json!({
        "deployId": "deploy_1",
        "deploymentUrl": "https://docs-0123456789-main.blobyard.app",
        "status": "superseded",
        "url": "https://docs-123456789-main.blobyard.app"
    }))
    .expect("deployment response");
    assert_eq!(deployment.deploy_id, "deploy_1");
}
