use super::super::support::{Fixture, api_failure, signed_server};
use super::super::transfer_fixtures::{completion, empty_reply, reservation};
use super::{SHA256, failed_deploy_response, start_response};
use blobyard_core::ErrorCode;

fn site() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("index.html"), b"abc").expect("index");
    root
}

fn deploy_args(directory: &str, scope: bool, public: bool) -> Vec<&str> {
    let mut args = vec!["blobyard"];
    if scope {
        args.extend(["--workspace", "team", "--project", "web"]);
    }
    args.extend(["deploy", directory, "--yard", "documentation"]);
    if public {
        args.push("--public");
    }
    args
}

#[tokio::test]
async fn deploy_propagates_selection_scope_and_public_lookup_failures() {
    let selection = Fixture::new(&["blobyard", "deploy", "--public"], Vec::new(), None, None);
    assert!(selection.runner.execute(&selection.command).await.is_err());

    let root = site();
    let directory = root.path().to_string_lossy();
    let scope = Fixture::new(
        &deploy_args(&directory, false, true),
        Vec::new(),
        None,
        None,
    );
    assert!(scope.runner.execute(&scope.command).await.is_err());

    let mut lookup = Fixture::new(
        &deploy_args(&directory, true, false),
        vec![api_failure(ErrorCode::ProviderUnavailable, "req_yards")],
        Some("ci-token"),
        None,
    );
    lookup.runner.set_test_confirmation(true, Ok(true));
    assert!(lookup.runner.execute(&lookup.command).await.is_err());
}

#[tokio::test]
async fn deploy_propagates_start_and_finalise_failures() {
    let root = site();
    let directory = root.path().to_string_lossy();
    let start = Fixture::new(
        &deploy_args(&directory, true, true),
        vec![api_failure(ErrorCode::ProviderUnavailable, "req_start")],
        Some("ci-token"),
        None,
    );
    assert!(start.runner.execute(&start.command).await.is_err());

    let (url, storage) = signed_server(vec![empty_reply("200 OK")]).await;
    let finalise = Fixture::new(
        &deploy_args(&directory, true, true),
        vec![
            start_response(
                "documentation",
                "deploy_started",
                ".blobyard-yard/yard_documentation/reserved/",
            ),
            reservation("single", &url, None, "upload_yard"),
            completion(3, SHA256),
            api_failure(ErrorCode::ProviderUnavailable, "req_finalise"),
            failed_deploy_response(),
        ],
        Some("ci-token"),
        None,
    );
    assert!(finalise.runner.execute(&finalise.command).await.is_err());
    assert_eq!(storage.await.expect("storage").len(), 1);
}

#[tokio::test]
async fn deploy_preserves_upload_failure_when_fail_notification_also_fails() {
    let root = site();
    let directory = root.path().to_string_lossy();
    let fixture = Fixture::new(
        &deploy_args(&directory, true, true),
        vec![
            start_response(
                "documentation",
                "deploy_started",
                ".blobyard-yard/yard_documentation/reserved/",
            ),
            api_failure(ErrorCode::PlanLimit, "req_upload"),
            api_failure(ErrorCode::ProviderUnavailable, "req_fail"),
        ],
        Some("ci-token"),
        None,
    );
    let error = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect_err("upload failure");
    assert_eq!(error.code(), ErrorCode::PlanLimit);
}
