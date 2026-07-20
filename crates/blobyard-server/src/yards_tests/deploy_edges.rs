use super::{super::deploy, super::lifecycle, faulted_state, request, upload_manifest};
use crate::{auth::Principal, error::ApiError, test_support::error_status, transfers::test_seams};
use axum::http::StatusCode;
use blobyard_api_client::{
    DeleteWebYardRequest, RollbackWebYardRequest, YardDeployMutationRequest,
};
use blobyard_contract::{WebYardRecord, YardDeployRecord, YardDeployStatus};

fn start(
    fixture: &test_seams::TransferFixture,
    principal: &Principal,
    client_deploy_id: &str,
    now: u64,
) -> (WebYardRecord, YardDeployRecord) {
    let _ = deploy::start(
        &fixture.state,
        principal,
        &request(client_deploy_id),
        Ok(now),
    )
    .expect("deploy start");
    let yard = fixture
        .state
        .repository
        .list_web_yards(&fixture.project.id)
        .expect("Yard list")
        .into_iter()
        .next()
        .expect("Yard");
    let deploy = fixture
        .state
        .repository
        .list_yard_deploys(&yard.id)
        .expect("deploy list")
        .into_iter()
        .find(|candidate| candidate.client_deploy_id == client_deploy_id)
        .expect("deploy");
    (yard, deploy)
}

async fn upload(fixture: &test_seams::TransferFixture, deploy: &YardDeployRecord, body: &[u8]) {
    upload_manifest(fixture, &deploy.manifest_root, body).await;
}

#[test]
fn start_propagates_clock_repository_and_response_failures() {
    let fixture = test_seams::fixture(&["yard:manage"]);
    let principal = Principal(fixture.principal.clone());
    assert_eq!(
        error_status(deploy::start(
            &fixture.state,
            &principal,
            &request("clock-start-0001"),
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(deploy::start(
            &faulted_state(&fixture, 2),
            &principal,
            &request("fault-start-0002"),
            Ok(1),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let mut invalid = fixture.state;
    invalid.web_yard_origin = "bad\norigin".to_owned();
    assert_eq!(
        error_status(deploy::start(
            &invalid,
            &principal,
            &request("origin-start-0003"),
            Ok(2),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn finalise_propagates_clock_and_every_repository_failure() {
    let fixture = test_seams::fixture(&["object:write", "yard:manage"]);
    let principal = Principal(fixture.principal.clone());
    let (_yard, candidate) = start(&fixture, &principal, "fault-finalise-0001", 1);
    upload(&fixture, &candidate, b"candidate").await;
    assert_eq!(
        error_status(deploy::finalise(
            &fixture.state,
            &principal,
            &YardDeployMutationRequest {
                deploy_id: candidate.id.clone(),
            },
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    for failure_index in 0..=5 {
        assert_eq!(
            error_status(deploy::finalise(
                &faulted_state(&fixture, failure_index),
                &principal,
                &YardDeployMutationRequest {
                    deploy_id: candidate.id.clone(),
                },
                Ok(2),
            )),
            StatusCode::INTERNAL_SERVER_ERROR,
            "failure index {failure_index}"
        );
    }
}

#[test]
fn fail_propagates_clock_and_every_repository_failure() {
    let fixture = test_seams::fixture(&["yard:manage"]);
    let principal = Principal(fixture.principal.clone());
    let (_yard, candidate) = start(&fixture, &principal, "fault-failure-0001", 1);
    let request = blobyard_api_client::FailYardDeployRequest {
        deploy_id: candidate.id,
        failure_code: "UPLOAD_FAILED".to_owned(),
        failure_message: "failed".to_owned(),
    };
    assert_eq!(
        error_status(deploy::fail(
            &fixture.state,
            &principal,
            &request,
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    for failure_index in 0..=3 {
        assert_eq!(
            error_status(deploy::fail(
                &faulted_state(&fixture, failure_index),
                &principal,
                &request,
                Ok(2),
            )),
            StatusCode::INTERNAL_SERVER_ERROR,
            "failure index {failure_index}"
        );
    }
}

#[tokio::test]
async fn inactive_yards_cannot_be_finalised() {
    let fixture = test_seams::fixture(&["object:write", "yard:manage"]);
    let principal = Principal(fixture.principal.clone());
    let (yard, deploy) = start(&fixture, &principal, "inactive-deploy-0001", 1);
    let _ = lifecycle::delete(
        &fixture.state,
        &principal,
        &DeleteWebYardRequest { yard_id: yard.id },
        Ok(2),
    )
    .expect("Yard deletion");
    assert_eq!(
        error_status(deploy::finalise(
            &fixture.state,
            &principal,
            &YardDeployMutationRequest {
                deploy_id: deploy.id,
            },
            Ok(3),
        )),
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn finalise_and_rollback_fail_when_the_public_origin_is_invalid() {
    let fixture = test_seams::fixture(&["object:write", "yard:manage"]);
    let principal = Principal(fixture.principal.clone());
    let (_yard, first) = start(&fixture, &principal, "origin-first-0001", 1);
    upload(&fixture, &first, b"first").await;
    let _ = deploy::finalise(
        &fixture.state,
        &principal,
        &YardDeployMutationRequest {
            deploy_id: first.id.clone(),
        },
        Ok(2),
    )
    .expect("first finalise");
    let (yard, second) = start(&fixture, &principal, "origin-second-0002", 3);
    upload(&fixture, &second, b"second").await;
    let mut invalid = fixture.state.clone();
    invalid.web_yard_origin = "bad\norigin".to_owned();
    assert_eq!(
        error_status(deploy::finalise(
            &invalid,
            &principal,
            &YardDeployMutationRequest {
                deploy_id: second.id.clone(),
            },
            Ok(4),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(lifecycle::rollback(
            &invalid,
            &principal,
            &RollbackWebYardRequest {
                yard_id: yard.id,
                deploy_id: Some(first.id),
            },
            Ok(5),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn equal_time_deploy_ids_select_the_newer_finalised_release() {
    let fixture = test_seams::fixture(&["object:write", "yard:manage"]);
    let principal = Principal(fixture.principal.clone());
    let (_yard, first) = start(&fixture, &principal, "same-time-first-0001", 10);
    let (_yard, second) = start(&fixture, &principal, "same-time-second-0002", 10);
    upload(&fixture, &first, b"first").await;
    upload(&fixture, &second, b"second").await;
    let mut ordered = [first, second];
    ordered.sort_by(|left, right| left.id.cmp(&right.id));
    let [lower, higher] = ordered;
    let _ = deploy::finalise(
        &fixture.state,
        &principal,
        &YardDeployMutationRequest {
            deploy_id: higher.id,
        },
        Ok(11),
    )
    .expect("higher deploy finalise");
    let lower_id = lower.id;
    let _ = deploy::finalise(
        &fixture.state,
        &principal,
        &YardDeployMutationRequest {
            deploy_id: lower_id.clone(),
        },
        Ok(12),
    )
    .expect("lower deploy finalise");
    assert_eq!(
        fixture
            .state
            .repository
            .yard_deploy_by_id(&lower_id)
            .expect("lower deploy")
            .status,
        YardDeployStatus::Superseded
    );
}
