use super::{
    super::read,
    faulted_state,
    operation_edge_tests::{list_query, yard},
    request,
};
use crate::{auth::Principal, test_support::error_status, transfers::test_seams};
use axum::http::StatusCode;
use blobyard_contract::{WebYardRecord, WebYardStatus};

fn started_yard(fixture: &test_seams::TransferFixture, principal: &Principal) -> WebYardRecord {
    let _ = super::super::deploy::start(
        &fixture.state,
        principal,
        &request("client-deploy-edge-0001"),
        Ok(1),
    )
    .expect("deploy start");
    fixture
        .state
        .repository
        .list_web_yards(&fixture.project.id)
        .expect("Yard list")
        .into_iter()
        .next()
        .expect("Yard")
}

#[test]
fn persisted_yard_relationships_fail_closed_for_foreign_authority_and_identity() {
    let fixture = test_seams::fixture(&["yard:manage"]);
    let principal = Principal(fixture.principal.clone());
    let persisted_yard = started_yard(&fixture, &principal);
    let mut persisted_deploy = fixture
        .state
        .repository
        .list_yard_deploys(&persisted_yard.id)
        .expect("deploy list")
        .into_iter()
        .next()
        .expect("deploy");
    let mut foreign_principal = principal.clone();
    foreign_principal.0.workspace_id = "workspace_foreign".to_owned();
    let deploy_query = blobyard_api_client::ListYardDeploysQuery {
        yard_id: persisted_yard.id,
    };
    assert_eq!(
        error_status(read::list_deploys(
            &fixture.state,
            &foreign_principal,
            &deploy_query,
        )),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        error_status(read::yard_for_deploy(
            &fixture.state,
            &foreign_principal,
            &persisted_deploy,
        )),
        StatusCode::NOT_FOUND
    );
    persisted_deploy.workspace_id = "workspace_foreign".to_owned();
    assert_eq!(
        error_status(read::yard_for_deploy(
            &fixture.state,
            &principal,
            &persisted_deploy,
        )),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn project_bound_yard_read_accepts_only_the_matching_project() {
    let fixture = test_seams::fixture(&["yard:read"]);
    let mut principal = Principal(fixture.principal);
    principal.0.project_id = Some("project_fixture".to_owned());
    read::authorize_yard(&principal, &yard(WebYardStatus::Active))
        .expect("matching project binding");
    principal.0.project_id = Some("project_foreign".to_owned());
    assert_eq!(
        error_status(read::authorize_yard(
            &principal,
            &yard(WebYardStatus::Active),
        )),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn environment_reads_conceal_foreign_yards_and_propagate_failures() {
    let fixture = test_seams::fixture(&["yard:read", "yard:manage"]);
    let principal = Principal(fixture.principal.clone());
    let persisted_yard = started_yard(&fixture, &principal);
    let environment_query = blobyard_api_client::ListYardEnvironmentsQuery {
        yard_id: persisted_yard.id,
    };
    let mut foreign_principal = principal.clone();
    foreign_principal.0.workspace_id = "workspace_foreign".to_owned();
    assert_eq!(
        error_status(read::list_environments(
            &fixture.state,
            &foreign_principal,
            &environment_query,
        )),
        StatusCode::NOT_FOUND
    );
    for failure_index in 0..=1 {
        assert_eq!(
            error_status(read::list_environments(
                &faulted_state(&fixture, failure_index),
                &principal,
                &environment_query,
            )),
            StatusCode::INTERNAL_SERVER_ERROR,
            "environment failure index {failure_index}"
        );
    }
    let _ = read::list_environments(&fixture.state, &principal, &environment_query)
        .expect("backfilled environments");
}

#[test]
fn read_operations_propagate_repository_and_presentation_failures() {
    let fixture = test_seams::fixture(&["yard:read", "yard:manage"]);
    let principal = Principal(fixture.principal.clone());
    let persisted_yard = started_yard(&fixture, &principal);
    let query = list_query();
    assert_eq!(
        error_status(read::list(&faulted_state(&fixture, 2), &principal, &query)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let deploy_query = blobyard_api_client::ListYardDeploysQuery {
        yard_id: persisted_yard.id,
    };
    for failure_index in 0..=1 {
        assert_eq!(
            error_status(read::list_deploys(
                &faulted_state(&fixture, failure_index),
                &principal,
                &deploy_query,
            )),
            StatusCode::INTERNAL_SERVER_ERROR,
            "failure index {failure_index}"
        );
    }
    let _ = read::list_deploys(&fixture.state, &principal, &deploy_query)
        .expect("uploading deploy history");
    let mut invalid = fixture.state;
    invalid.web_yard_origin = "bad\norigin".to_owned();
    assert_eq!(
        error_status(read::list(&invalid, &principal, &query)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(read::list_deploys(&invalid, &principal, &deploy_query)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
