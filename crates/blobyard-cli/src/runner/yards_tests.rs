#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    deploy_lines, deploy_status, ensure_unpaginated, invalid_deploy_id, named_yard, select_yard,
    unexpected_cursor, validate_deploy_id, yard_lines,
};
use blobyard_api_client::{Page, WebYardSummary, YardDeployStatus, YardDeploySummary};
use blobyard_core::{ErrorCode, Slug};

fn yard(name: &str) -> WebYardSummary {
    serde_json::from_value(serde_json::json!({
        "currentDeployId": null,
        "hostLabel": format!("{name}-abc234ef"),
        "id": format!("yard_{name}"),
        "name": name,
        "projectId": "project_1",
        "status": "active",
        "url": format!("https://{name}-abc234ef.blobyard.app"),
        "workspaceId": "workspace_1"
    }))
    .expect("yard")
}

#[test]
fn selection_and_empty_presentations_cover_each_outcome() {
    let documentation = yard("documentation");
    let marketing = yard("marketing");
    let yards = [documentation, marketing];
    assert_eq!(
        select_yard(&yards, Some("documentation"))
            .expect("selected")
            .name
            .as_str(),
        "documentation"
    );
    let missing = Slug::new("missing").expect("slug");
    assert_eq!(
        named_yard(&yards, &missing).expect_err("missing").code(),
        ErrorCode::NotFound
    );
    assert!(select_yard(&yards, None).is_err());
    assert!(select_yard(&[], None).is_err());
    assert_eq!(
        select_yard(&yards, Some("api"))
            .expect_err("reserved name")
            .code(),
        ErrorCode::InvalidRequest
    );
    assert_eq!(yard_lines(&[]), "No Web Yards found.");
    assert_eq!(deploy_lines(&[]), "No Web Yard deploys found.");
}

#[test]
fn status_and_validation_helpers_cover_every_lifecycle_state() {
    assert_eq!(deploy_status(YardDeployStatus::Uploading), "uploading");
    assert_eq!(deploy_status(YardDeployStatus::Finalising), "finalising");
    assert_eq!(deploy_status(YardDeployStatus::Live), "live");
    assert_eq!(deploy_status(YardDeployStatus::Failed), "failed");
    assert_eq!(deploy_status(YardDeployStatus::Superseded), "superseded");
    assert_eq!(deploy_status(YardDeployStatus::Pruned), "pruned");
    let uploading: YardDeploySummary = serde_json::from_value(serde_json::json!({
        "cleanUrls": false, "clientDeployId": "client_1", "createdAt": 1,
        "deploymentUrl": "https://documentation-0123456789-team.blobyard.app",
        "fileCount": 0, "finalisedAt": null, "id": "deploy_1", "isCurrent": false,
        "spa": false, "status": "uploading", "totalBytes": 0
    }))
    .expect("deploy");
    let line = deploy_lines(&[uploading]);
    assert!(line.contains("not finalised"));
    assert!(line.contains("https://documentation-0123456789-team.blobyard.app"));
    assert!(!line.contains(" *"));
}

#[test]
fn validation_helpers_reject_ambiguous_remote_and_local_inputs() {
    assert!(validate_deploy_id(None).is_ok());
    assert!(validate_deploy_id(Some("deploy_1")).is_ok());
    assert_eq!(
        validate_deploy_id(Some("")).expect_err("empty").code(),
        ErrorCode::InvalidRequest
    );
    assert_eq!(invalid_deploy_id().code(), ErrorCode::InvalidRequest);
    assert_eq!(unexpected_cursor().code(), ErrorCode::ProviderUnavailable);

    let page: Page<WebYardSummary> = serde_json::from_value(serde_json::json!({
        "items": [], "nextCursor": "next"
    }))
    .expect("page");
    assert_eq!(
        ensure_unpaginated(&page).expect_err("cursor").code(),
        ErrorCode::ProviderUnavailable
    );
}
