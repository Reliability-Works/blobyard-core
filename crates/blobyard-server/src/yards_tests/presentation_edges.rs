use super::{
    super::presentation,
    operation_edge_tests::{deploy, yard},
};
use crate::test_support::error_status;
use axum::http::StatusCode;
use blobyard_api_client::{WebYardStatus as ApiYardStatus, YardDeployStatus as ApiDeployStatus};
use blobyard_contract::{WebYardStatus, YardDeployStatus, YardDeploymentRecord, YardStartRecord};

#[test]
fn presentation_maps_every_persisted_yard_and_deploy_status() {
    let origin = "http://localhost:8787";
    let suspended = presentation::yard_summary(origin, yard(WebYardStatus::Suspended))
        .expect("suspended summary");
    assert_eq!(suspended.status, ApiYardStatus::Suspended);
    assert_eq!(
        error_status(presentation::yard_summary(
            origin,
            yard(WebYardStatus::Deleted)
        )),
        StatusCode::NOT_FOUND
    );
    for (status, expected) in [
        (YardDeployStatus::Finalising, ApiDeployStatus::Finalising),
        (YardDeployStatus::Failed, ApiDeployStatus::Failed),
        (YardDeployStatus::Pruned, ApiDeployStatus::Pruned),
    ] {
        assert_eq!(
            presentation::deploy_summary(origin, deploy(status), None)
                .expect("deploy summary")
                .status,
            expected
        );
    }
}

#[test]
fn mutation_responses_reject_a_corrupt_stable_host_after_a_valid_deployment_host() {
    let mut corrupt_yard = yard(WebYardStatus::Active);
    corrupt_yard.host_label = "invalid host".to_owned();
    let uploading = deploy(YardDeployStatus::Uploading);
    assert_eq!(
        error_status(presentation::start_response(
            "http://localhost:8787",
            YardStartRecord {
                yard: corrupt_yard.clone(),
                deploy: uploading,
            },
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(presentation::deployment_response(
            "http://localhost:8787",
            YardDeploymentRecord {
                yard: corrupt_yard,
                deploy: deploy(YardDeployStatus::Live),
            },
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
