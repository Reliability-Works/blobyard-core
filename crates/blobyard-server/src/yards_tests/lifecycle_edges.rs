use super::{super::lifecycle, faulted_state, mutate, start, upload_manifest};
use crate::{
    api::AppState, auth::Principal, error::ApiError, test_support::error_status,
    transfers::test_seams,
};
use axum::http::StatusCode;
use blobyard_api_client::{DeleteWebYardRequest, RollbackWebYardRequest};
use std::ops::RangeInclusive;

async fn rollback_fixture() -> (test_seams::TransferFixture, String, String) {
    let fixture = test_seams::fixture(&["object:write", "yard:manage"]);
    let first = start(&fixture, "lifecycle-first-0001").await;
    let root = first["data"]["manifestRoot"]
        .as_str()
        .expect("first manifest root");
    upload_manifest(&fixture, root, b"first").await;
    let first_id = first["data"]["deployId"]
        .as_str()
        .expect("first deploy")
        .to_owned();
    let yard_id = first["data"]["yardId"].as_str().expect("Yard").to_owned();
    let _ = mutate(
        &fixture,
        "/v1/yards/deploys/finalise",
        serde_json::json!({"deployId": first_id}),
    )
    .await;
    let second = start(&fixture, "lifecycle-second-0002").await;
    let root = second["data"]["manifestRoot"]
        .as_str()
        .expect("second manifest root");
    upload_manifest(&fixture, root, b"second").await;
    let second_id = second["data"]["deployId"].as_str().expect("second deploy");
    let _ = mutate(
        &fixture,
        "/v1/yards/deploys/finalise",
        serde_json::json!({"deployId": second_id}),
    )
    .await;
    (fixture, yard_id, first_id)
}

fn assert_mutation_failures<T>(
    fixture: &test_seams::TransferFixture,
    principal: &Principal,
    repository_failures: RangeInclusive<usize>,
    successful_time: u64,
    operation: impl Fn(&AppState, &Principal, Result<u64, ApiError>) -> Result<T, ApiError>,
) {
    assert_eq!(
        error_status(operation(
            &fixture.state,
            principal,
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let mut foreign = principal.clone();
    foreign.0.workspace_id = "workspace_foreign".to_owned();
    assert_eq!(
        error_status(operation(&fixture.state, &foreign, Ok(successful_time))),
        StatusCode::NOT_FOUND
    );
    for failure_index in repository_failures {
        assert_eq!(
            error_status(operation(
                &faulted_state(fixture, failure_index),
                principal,
                Ok(successful_time),
            )),
            StatusCode::INTERNAL_SERVER_ERROR,
            "failure index {failure_index}"
        );
    }
}

#[tokio::test]
async fn rollback_propagates_clock_authorization_and_every_repository_failure() {
    let (fixture, yard_id, first_id) = rollback_fixture().await;
    let principal = Principal(fixture.principal.clone());
    let request = RollbackWebYardRequest {
        yard_id,
        deploy_id: Some(first_id),
    };
    assert_mutation_failures(&fixture, &principal, 0..=2, 10, |state, principal, now| {
        lifecycle::rollback(state, principal, &request, now)
    });
}

#[tokio::test]
async fn delete_propagates_clock_authorization_and_every_repository_failure() {
    let fixture = test_seams::fixture(&["object:write", "yard:manage"]);
    let started = start(&fixture, "lifecycle-delete-0001").await;
    let root = started["data"]["manifestRoot"]
        .as_str()
        .expect("manifest root");
    upload_manifest(&fixture, root, b"delete fixture").await;
    let deploy_id = started["data"]["deployId"].as_str().expect("deploy");
    let _ = mutate(
        &fixture,
        "/v1/yards/deploys/finalise",
        serde_json::json!({"deployId": deploy_id}),
    )
    .await;
    let request = DeleteWebYardRequest {
        yard_id: started["data"]["yardId"].as_str().expect("Yard").to_owned(),
    };
    let principal = Principal(fixture.principal.clone());
    let deleted_at = crate::transfer_grants::now_ms().expect("deletion time");
    assert_mutation_failures(
        &fixture,
        &principal,
        0..=2,
        deleted_at,
        |state, principal, now| lifecycle::delete(state, principal, &request, now),
    );
}
