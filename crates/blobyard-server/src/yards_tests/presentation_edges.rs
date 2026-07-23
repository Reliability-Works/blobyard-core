use super::{
    super::presentation,
    operation_edge_tests::{deploy, yard},
};
use crate::test_support::error_status;
use axum::http::StatusCode;
use blobyard_api_client::{
    WebYardStatus as ApiYardStatus, YardDeployStatus as ApiDeployStatus,
    YardEnvironmentKind as ApiEnvironmentKind,
};
use blobyard_contract::{
    WebYardStatus, YardDeployStatus, YardDeploymentRecord, YardEnvironmentKind,
    YardEnvironmentRecord, YardEnvironmentStatus, YardStartRecord,
};
use blobyard_core::Slug;

fn environment(kind: YardEnvironmentKind) -> YardEnvironmentRecord {
    YardEnvironmentRecord {
        id: "yardenv_yard_edge".to_owned(),
        yard_id: "yard_edge".to_owned(),
        name: Slug::new("production").expect("environment name"),
        kind,
        status: YardEnvironmentStatus::Active,
        created_at_ms: 1,
        updated_at_ms: 2,
    }
}

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
fn environment_summaries_map_every_kind_with_stable_timestamps() {
    for (kind, expected) in [
        (
            YardEnvironmentKind::Production,
            ApiEnvironmentKind::Production,
        ),
        (YardEnvironmentKind::Staging, ApiEnvironmentKind::Staging),
        (YardEnvironmentKind::Preview, ApiEnvironmentKind::Preview),
    ] {
        let summary =
            presentation::environment_summary(environment(kind)).expect("environment summary");
        assert_eq!(summary.kind, expected);
        assert_eq!(summary.created_at, "1970-01-01T00:00:00.001Z");
        assert_eq!(summary.updated_at, "1970-01-01T00:00:00.002Z");
    }
}

#[test]
fn environment_summaries_reject_unrepresentable_timestamps() {
    let mut created = environment(YardEnvironmentKind::Production);
    created.created_at_ms = u64::MAX;
    created.updated_at_ms = u64::MAX;
    assert_eq!(
        error_status(presentation::environment_summary(created)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let mut updated = environment(YardEnvironmentKind::Production);
    updated.updated_at_ms = u64::MAX;
    assert_eq!(
        error_status(presentation::environment_summary(updated)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
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
