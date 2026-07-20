#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::super::deploy_selection::SelectedYard;
use super::{manifest_prefix, until_interrupted, valid_host_label, validate_start};
use blobyard_api_client::{StartYardDeployResponse, YardDeployStatus, YardDeploymentResponse};
use blobyard_core::{ErrorCode, Slug, WebYardOrigin};
use futures_util::future::pending;
use std::path::PathBuf;

fn selected() -> SelectedYard {
    SelectedYard {
        name: Slug::new("documentation").expect("yard"),
        directory: PathBuf::from("dist"),
        spa: false,
        clean_urls: true,
    }
}

fn started() -> StartYardDeployResponse {
    StartYardDeployResponse {
        deploy_id: "deploy_1".into(),
        deployment_url: "https://documentation-0123456789-team.blobyard.app".into(),
        host_label: "documentation-123456789-team".into(),
        manifest_root: ".blobyard-yard/yard_1/client_1/".into(),
        status: YardDeployStatus::Uploading,
        url: "https://documentation-123456789-team.blobyard.app".into(),
        yard_id: "yard_1".into(),
        yard_name: Slug::new("documentation").expect("yard"),
    }
}

fn origin() -> WebYardOrigin {
    WebYardOrigin::new("https://blobyard.app").expect("yard origin")
}

#[tokio::test]
async fn interruption_race_preserves_completion_or_returns_interrupted() {
    let completed = until_interrupted(
        Box::pin(async {
            Ok((
                YardDeploymentResponse {
                    deploy_id: "deploy_1".into(),
                    deployment_url: "https://documentation-0123456789-team.blobyard.app".into(),
                    status: YardDeployStatus::Live,
                    url: "https://documentation-123456789-team.blobyard.app".into(),
                },
                "req_1".into(),
            ))
        }),
        Box::pin(pending::<std::io::Result<()>>()),
    )
    .await;
    assert_eq!(completed.expect("completed").0.deploy_id, "deploy_1");
    let interrupted = until_interrupted(
        Box::pin(pending()),
        Box::pin(async { Ok::<(), std::io::Error>(()) }),
    )
    .await;
    assert_eq!(
        interrupted.expect_err("interrupted").code(),
        ErrorCode::Interrupted
    );
}

#[test]
fn start_metadata_validation_accepts_only_the_isolated_yard_contract() {
    let selected = selected();
    let valid = started();
    assert!(validate_start(&valid, &selected, &origin()).is_ok());
    assert_eq!(
        manifest_prefix(&valid),
        Some(".blobyard-yard/yard_1/client_1")
    );
    assert!(valid_host_label("documentation-123456789-team"));

    for host in [
        "missingseparator",
        "-team",
        "team-",
        "UPPER-team",
        "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijkl-team",
    ] {
        assert!(!valid_host_label(host), "accepted {host}");
    }

    for manifest in [
        ".blobyard-yard/yard_1/client_1",
        ".wrong-root/yard_1/client_1/",
        ".blobyard-yard/yard_1/",
        ".blobyard-yard/wrong/client_1/",
        ".blobyard-yard/yard_1//",
        ".blobyard-yard/yard_1/client/extra/",
        ".blobyard-yard/yard_1/client\n/",
    ] {
        let mut response = valid.clone();
        response.manifest_root = manifest.into();
        assert!(
            manifest_prefix(&response).is_none(),
            "accepted {manifest:?}"
        );
    }
}

#[test]
fn start_metadata_validation_accepts_an_explicit_self_hosted_origin_only() {
    let selected = selected();
    let origin = WebYardOrigin::new("https://yards.example.test").expect("yard origin");
    let mut response = started();
    response.url = "https://documentation-123456789-team.yards.example.test".into();
    response.deployment_url = "https://documentation-0123456789-team.yards.example.test".into();
    assert!(validate_start(&response, &selected, &origin).is_ok());

    response.url = "https://documentation-123456789-team.blobyard.app".into();
    assert!(validate_start(&response, &selected, &origin).is_err());
}

#[test]
fn start_metadata_validation_rejects_each_inconsistent_identity_field() {
    let selected = selected();
    let mut cases = Vec::new();
    let mut response = started();
    response.yard_name = Slug::new("marketing").expect("yard");
    cases.push(response);
    let mut response = started();
    response.status = YardDeployStatus::Live;
    cases.push(response);
    let mut response = started();
    response.deploy_id.clear();
    cases.push(response);
    let mut response = started();
    response.host_label = "invalid".into();
    cases.push(response);
    let mut response = started();
    response.url = "https://example.com".into();
    cases.push(response);
    let mut response = started();
    response.url = "documentation-123456789-team.blobyard.app".into();
    cases.push(response);
    let mut response = started();
    response.deployment_url = "https://example.com".into();
    cases.push(response);
    let mut response = started();
    response.deployment_url = "documentation-0123456789-team.blobyard.app".into();
    cases.push(response);
    let mut response = started();
    response.manifest_root = ".blobyard-yard/wrong/client_1/".into();
    cases.push(response);
    assert!(
        cases
            .iter()
            .all(|response| validate_start(response, &selected, &origin()).is_err())
    );
}
