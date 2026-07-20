use super::super::support::{Fixture, ok, signed_server};
use super::super::transfer_fixtures::{completion, empty_reply, reservation};
use super::{SHA256, deployment_response, start_response, yard};
use blobyard_api_client::Endpoint;
use blobyard_core::{BlobyardError, ErrorCode};

#[tokio::test]
async fn interactive_deploy_confirms_only_the_first_publication() {
    successful_interactive_deploy(None).await;
    successful_interactive_deploy(Some("deploy_previous")).await;
}

async fn successful_interactive_deploy(current: Option<&str>) {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("index.html"), b"abc").expect("index");
    let (url, storage) = signed_server(vec![empty_reply("200 OK")]).await;
    let yards = current.map_or_else(Vec::new, |deploy| vec![yard("documentation", Some(deploy))]);
    let mut fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "deploy",
            &root.path().to_string_lossy(),
            "--yard",
            "documentation",
        ],
        vec![
            ok(
                serde_json::json!({ "items": yards, "nextCursor": null }),
                "req_yards",
            ),
            start_response(
                "documentation",
                "deploy_started",
                ".blobyard-yard/yard_documentation/reserved/",
            ),
            reservation("single", &url, None, "upload_yard"),
            completion(3, SHA256),
            deployment_response("documentation", "deploy_started", "live"),
        ],
        Some("ci-token"),
        None,
    );
    fixture.runner.set_test_confirmation(true, Ok(true));
    fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("interactive deploy");
    assert_eq!(
        fixture.transport.requests()[0].endpoint(),
        Endpoint::ListWebYards
    );
    assert_eq!(storage.await.expect("storage").len(), 1);
}

#[tokio::test]
async fn interactive_deploy_preserves_decline_and_confirmation_errors() {
    let declined = interactive_failure(Ok(false));
    let error = declined.await.expect_err("declined");
    assert_eq!(error.code(), ErrorCode::Interrupted);

    let failure = BlobyardError::from_code(ErrorCode::InternalError);
    let error = interactive_failure(Err(failure))
        .await
        .expect_err("confirmation failure");
    assert_eq!(error.code(), ErrorCode::InternalError);
}

async fn interactive_failure(
    confirmation: Result<bool, BlobyardError>,
) -> Result<blobyard_cli::CommandResult, BlobyardError> {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("index.html"), b"abc").expect("index");
    let mut fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "deploy",
            &root.path().to_string_lossy(),
            "--yard",
            "documentation",
        ],
        vec![ok(
            serde_json::json!({ "items": [], "nextCursor": null }),
            "req_yards",
        )],
        Some("ci-token"),
        None,
    );
    fixture.runner.set_test_confirmation(true, confirmation);
    fixture.runner.execute(&fixture.command).await
}
