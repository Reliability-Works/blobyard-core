use super::{
    super::contracts, super::public_fallback, super::read, super::require_read, public_request,
    request,
};
use crate::{auth::Principal, test_support::error_status, transfers::test_seams};
use axum::{
    extract::{OriginalUri, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
};
use blobyard_api_client::ListWebYardsQuery;
use blobyard_contract::{WebYardRecord, WebYardStatus, YardDeployRecord, YardDeployStatus};
use blobyard_core::Slug;

pub(super) fn yard(status: WebYardStatus) -> WebYardRecord {
    WebYardRecord {
        id: "yard_edge".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: Slug::new("edge").expect("yard"),
        host_label: "edge-123456789-fixture".to_owned(),
        current_deploy_id: Some("deploy_edge".to_owned()),
        status,
        created_at_ms: 1,
        updated_at_ms: 2,
        deleted_at_ms: None,
    }
}

pub(super) fn deploy(status: YardDeployStatus) -> YardDeployRecord {
    YardDeployRecord {
        id: "deploy_edge".to_owned(),
        yard_id: "yard_edge".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        client_deploy_id: "client-deploy-edge-0001".to_owned(),
        manifest_root: ".blobyard-yard/yard_edge/client-deploy-edge-0001/".to_owned(),
        deployment_host_label: "edge-0123456789-fixture".to_owned(),
        spa: true,
        clean_urls: true,
        status,
        created_at_ms: 1,
        finalised_at_ms: None,
        file_count: 0,
        total_bytes: 0,
    }
}

pub(super) fn list_query() -> ListWebYardsQuery {
    ListWebYardsQuery {
        workspace: Slug::new("fixture").expect("workspace"),
        project: Slug::new("project").expect("project"),
    }
}

#[test]
fn project_resolution_and_binding_fail_closed() {
    let fixture = test_seams::fixture(&["yard:manage"]);
    let principal = Principal(fixture.principal.clone());
    let missing = ListWebYardsQuery {
        workspace: Slug::new("missing").expect("workspace"),
        project: Slug::new("project").expect("project"),
    };
    assert_eq!(
        error_status(read::list(&fixture.state, &principal, &missing)),
        StatusCode::NOT_FOUND
    );
    let mut missing_start = request("client-deploy-edge-0001");
    missing_start.workspace = missing.workspace;
    assert_eq!(
        error_status(super::super::deploy::start(
            &fixture.state,
            &principal,
            &missing_start,
            Ok(1),
        )),
        StatusCode::NOT_FOUND
    );
    let mut bound = principal;
    bound.0.project_id = Some("project_foreign".to_owned());
    let query = list_query();
    assert_eq!(
        error_status(read::list(&fixture.state, &bound, &query)),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        error_status(super::super::deploy::start(
            &fixture.state,
            &bound,
            &request("client-deploy-edge-0001"),
            Ok(1),
        )),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn foreign_and_corrupt_yard_relationships_fail_closed() {
    let fixture = test_seams::fixture(&["yard:manage"]);
    let principal = Principal(fixture.principal.clone());
    let mut foreign = yard(WebYardStatus::Active);
    foreign.workspace_id = "workspace_foreign".to_owned();
    assert_eq!(
        error_status(read::authorize_yard(&principal, &foreign)),
        StatusCode::NOT_FOUND
    );
    let mut corrupt = deploy(YardDeployStatus::Uploading);
    corrupt.workspace_id = "workspace_foreign".to_owned();
    assert_eq!(
        error_status(read::yard_for_deploy(&fixture.state, &principal, &corrupt)),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn machine_read_requires_the_yard_manage_action() {
    let fixture = test_seams::fixture(&["yard:read"]);
    let mut machine = Principal(fixture.principal);
    machine.0.id = "machine_fixture".to_owned();
    assert_eq!(error_status(require_read(&machine)), StatusCode::FORBIDDEN);
    machine.0.scopes = vec!["yard:manage".to_owned()];
    require_read(&machine).expect("machine Yard authority");
}

#[test]
fn public_request_contract_rejects_oversized_normalized_paths() {
    let path = format!(
        "/{}",
        "a".repeat(blobyard_contract::MAXIMUM_YARD_PATH_BYTES + 1)
    );
    assert_eq!(
        error_status(contracts::public_request_path(&path)),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn recognized_yard_hosts_conceal_unsupported_methods() {
    let fixture = test_seams::fixture(&["yard:read"]);
    assert_eq!(
        public_request(
            &fixture,
            "POST",
            "/",
            "site-123456789-fixture.localhost",
            None,
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn recognized_yard_hosts_conceal_invalid_request_paths() {
    let fixture = test_seams::fixture(&["yard:read"]);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::HOST,
        HeaderValue::from_static("site-123456789-fixture.localhost:8787"),
    );
    assert_eq!(
        error_status(
            public_fallback(
                State(fixture.state),
                OriginalUri(Uri::from_static("/bad//path")),
                Method::GET,
                headers,
            )
            .await,
        ),
        StatusCode::NOT_FOUND
    );
}
