//! Local capability orchestration failure boundaries.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, api_failure, signed_server};
use super::transfer_fixtures::{completion, empty_reply, reservation};
use blobyard_core::ErrorCode;

const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

#[tokio::test]
async fn preview_rejects_duration_and_missing_scope_before_uploading() {
    let root = preview_root();
    let directory = root.path().to_string_lossy();
    for arguments in [
        vec!["blobyard", "preview", &directory, "--expires", "never"],
        vec!["blobyard", "preview", &directory, "--workspace", "team"],
    ] {
        let fixture = Fixture::new(&arguments, Vec::new(), Some("ci-token"), None);
        assert_eq!(execute_error(&fixture).await, ErrorCode::InvalidRequest);
        assert!(fixture.transport.requests().is_empty());
    }
}

#[tokio::test]
async fn preview_stops_on_upload_or_capability_api_failure() {
    let root = preview_root();
    let directory = root.path().to_string_lossy().into_owned();
    let upload_failure = Fixture::new(
        &preview_command(&directory),
        vec![api_failure(ErrorCode::Forbidden, "req_upload")],
        Some("ci-token"),
        None,
    );
    assert_eq!(execute_error(&upload_failure).await, ErrorCode::Forbidden);
    assert_eq!(upload_failure.transport.requests().len(), 1);

    let (url, storage) = signed_server(vec![empty_reply("200 OK")]).await;
    let preview_failure = Fixture::new(
        &preview_command(&directory),
        vec![
            reservation("single", &url, None, "upload_preview"),
            completion(3, ABC_SHA256),
            api_failure(ErrorCode::ProviderUnavailable, "req_preview"),
        ],
        Some("ci-token"),
        None,
    );
    assert_eq!(
        execute_error(&preview_failure).await,
        ErrorCode::ProviderUnavailable
    );
    storage.await.expect("storage");
}

#[tokio::test]
async fn local_share_stops_when_upload_reservation_fails() {
    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("artifact.bin");
    std::fs::write(&source, b"abc").expect("source");
    let fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "app",
            "share",
            &source.to_string_lossy(),
        ],
        vec![api_failure(ErrorCode::PlanLimit, "req_limit")],
        Some("ci-token"),
        None,
    );
    assert_eq!(execute_error(&fixture).await, ErrorCode::PlanLimit);
    assert_eq!(fixture.transport.requests().len(), 1);
}

async fn execute_error(fixture: &Fixture) -> ErrorCode {
    fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect_err("capability failure")
        .code()
}

fn preview_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("index.html"), b"abc").expect("index");
    root
}

fn preview_command(directory: &str) -> [&str; 9] {
    [
        "blobyard",
        "--workspace",
        "team",
        "--project",
        "app",
        "preview",
        directory,
        "--expires",
        "24h",
    ]
}
