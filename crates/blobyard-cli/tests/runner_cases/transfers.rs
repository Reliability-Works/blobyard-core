//! Complete direct-transfer runner workflows over local API and storage seams.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{
    Fixture, SignedReply, api_failure, ok, result_json, signed_server, signed_server_with_action,
};
use blobyard_api_client::Endpoint;
use blobyard_core::ErrorCode;

const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const ABCDEF_SHA256: &str = "bef57ec7f53a6d40beb640a780a639c83bc29ac8a9816f1c5c6dcd93c4721f33";

#[path = "transfers_contract.rs"]
mod contract;
#[path = "transfers_failures.rs"]
mod failures;
#[path = "transfers_resume.rs"]
mod resume;

use super::transfer_fixtures::{completion, empty_reply, etag_reply, part_grants, reservation};

#[tokio::test]
async fn single_upload_streams_bytes_and_completes() {
    let source_root = tempfile::tempdir().expect("source root");
    let source = source_root.path().join("artifact.bin");
    std::fs::write(&source, b"abc").expect("source");
    let (url, storage) = signed_server(vec![empty_reply("200 OK")]).await;
    let source_text = source.to_string_lossy().into_owned();
    let fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "app",
            "upload",
            &source_text,
            "--path",
            "builds/artifact.bin",
        ],
        vec![
            reservation("single", &url, None, "upload_1"),
            completion(3, ABC_SHA256),
        ],
        Some("ci-token"),
        None,
    );
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("upload");
    assert_eq!(result_json(result)["data"]["files"][0]["sizeBytes"], 3);
    let requests = fixture.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::RequestUpload);
    assert_eq!(
        requests[0].body().expect("body")["path"],
        "builds/artifact.bin"
    );
    assert_eq!(requests[1].endpoint(), Endpoint::CompleteUpload);
    let storage_requests = storage.await.expect("storage");
    assert!(storage_requests[0].windows(3).any(|part| part == b"abc"));
}

#[tokio::test]
async fn failed_single_upload_aborts_server_reservation() {
    let source_root = tempfile::tempdir().expect("source root");
    let source = source_root.path().join("artifact.bin");
    std::fs::write(&source, b"abc").expect("source");
    let (url, storage) = signed_server(vec![empty_reply("400 Bad Request")]).await;
    let source_text = source.to_string_lossy().into_owned();
    let fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "app",
            "upload",
            &source_text,
        ],
        vec![
            reservation("single", &url, None, "upload_abort"),
            ok(serde_json::json!({}), "req_abort"),
        ],
        Some("ci-token"),
        None,
    );
    assert_eq!(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("storage failure")
            .code(),
        ErrorCode::StorageError
    );
    assert_eq!(
        fixture.transport.requests()[1].endpoint(),
        Endpoint::AbortUpload
    );
    storage.await.expect("storage");
}

#[tokio::test]
async fn download_is_verified_atomic_and_force_gated() {
    let destination_root = tempfile::tempdir().expect("destination root");
    let destination = destination_root.path().join("artifact.bin");
    std::fs::write(&destination, b"old").expect("existing");
    let output = destination.to_string_lossy().into_owned();
    let conflict = Fixture::new(
        &[
            "blobyard",
            "download",
            "blobyard://team/app/artifact.bin",
            "--output",
            &output,
        ],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    assert_eq!(
        conflict
            .runner
            .execute(&conflict.command)
            .await
            .expect_err("overwrite blocked")
            .code(),
        ErrorCode::Conflict
    );
    assert!(conflict.transport.requests().is_empty());

    let (url, storage) = signed_server(vec![SignedReply {
        status: "200 OK",
        headers: Vec::new(),
        body: b"abc".to_vec(),
    }])
    .await;
    let forced = Fixture::new(
        &[
            "blobyard",
            "download",
            "blobyard://team/app/artifact.bin",
            "--output",
            &output,
            "--force",
        ],
        vec![download_grant(&url, ABC_SHA256)],
        Some("ci-token"),
        None,
    );
    forced
        .runner
        .execute(&forced.command)
        .await
        .expect("download");
    assert_eq!(std::fs::read(&destination).expect("download bytes"), b"abc");
    storage.await.expect("storage");
}

#[tokio::test]
async fn download_integrity_failure_keeps_existing_destination() {
    let root = tempfile::tempdir().expect("root");
    let destination = root.path().join("artifact.bin");
    std::fs::write(&destination, b"safe-old").expect("existing");
    let output = destination.to_string_lossy().into_owned();
    let (url, storage) = signed_server(vec![SignedReply {
        status: "200 OK",
        headers: Vec::new(),
        body: b"tampered".to_vec(),
    }])
    .await;
    let fixture = Fixture::new(
        &[
            "blobyard",
            "download",
            "blobyard://team/app/artifact.bin",
            "--output",
            &output,
            "--force",
        ],
        vec![download_grant(&url, ABC_SHA256)],
        Some("ci-token"),
        None,
    );
    assert_eq!(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("integrity")
            .code(),
        ErrorCode::ChecksumMismatch
    );
    assert_eq!(std::fs::read(destination).expect("existing"), b"safe-old");
    storage.await.expect("storage");
}

#[tokio::test]
async fn download_validation_api_and_storage_failures_are_distinct() {
    let root = tempfile::tempdir().expect("root");
    let invalid_output = root.path().join("invalid").to_string_lossy().into_owned();
    let invalid = download_fixture("not-a-uri", &invalid_output, Vec::new());
    assert_eq!(execute_error(&invalid).await, ErrorCode::InvalidRequest);

    let api_output = root.path().join("api.bin").to_string_lossy().into_owned();
    let api = download_fixture(
        "blobyard://team/app/a.bin",
        &api_output,
        vec![api_failure(ErrorCode::Forbidden, "req_forbidden")],
    );
    assert_eq!(execute_error(&api).await, ErrorCode::Forbidden);

    let bad_parent = root
        .path()
        .join("missing/out.bin")
        .to_string_lossy()
        .into_owned();
    let parent = download_fixture(
        "blobyard://team/app/a.bin",
        &bad_parent,
        vec![download_grant("http://127.0.0.1:1/signed", ABC_SHA256)],
    );
    assert_eq!(execute_error(&parent).await, ErrorCode::StorageError);

    let (url, storage) = signed_server(vec![empty_reply("404 Not Found")]).await;
    let storage_output = root
        .path()
        .join("storage.bin")
        .to_string_lossy()
        .into_owned();
    let storage_failure = download_fixture(
        "blobyard://team/app/a.bin",
        &storage_output,
        vec![download_grant(&url, ABC_SHA256)],
    );
    assert_eq!(
        execute_error(&storage_failure).await,
        ErrorCode::StorageError
    );
    storage.await.expect("storage");
}

async fn execute_error(fixture: &Fixture) -> ErrorCode {
    fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect_err("runner failure")
        .code()
}

fn download_fixture(
    uri: &str,
    output: &str,
    responses: Vec<blobyard_api_client::RawResponse>,
) -> Fixture {
    Fixture::new(
        &["blobyard", "download", uri, "--output", output],
        responses,
        Some("ci-token"),
        None,
    )
}

fn upload_command(source: &str) -> [&str; 7] {
    [
        "blobyard",
        "--workspace",
        "team",
        "--project",
        "app",
        "upload",
        source,
    ]
}

fn download_grant(url: &str, checksum: &str) -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({
            "downloadUrl": url,
            "filename": "artifact.bin",
            "sizeBytes": 3,
            "checksumSha256": checksum,
            "expiresAt": "2030-01-01T00:00:00Z"
        }),
        "req_download",
    )
}
