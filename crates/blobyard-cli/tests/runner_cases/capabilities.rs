//! Local-path share and immutable static-preview workflows.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, ok, result_json, signed_server};
use super::transfer_fixtures::{completion, empty_reply, reservation};
use blobyard_api_client::Endpoint;
use blobyard_core::ErrorCode;

const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const SHARE_URL: &str = "https://blobyard.com/s/raw-once";
const PREVIEW_URL: &str =
    "https://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.blobyard.dev/";

#[tokio::test]
async fn local_file_share_streams_then_shares_the_completed_immutable_uri() {
    let root = tempfile::tempdir().expect("root");
    let source = root.path().join("artifact.bin");
    std::fs::write(&source, b"abc").expect("source");
    let (url, storage) = signed_server(vec![empty_reply("200 OK")]).await;
    let fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "app",
            "share",
            &source.to_string_lossy(),
            "--expires",
            "7d",
        ],
        vec![
            reservation("single", &url, None, "upload_share"),
            completion(3, ABC_SHA256),
            share_response(),
        ],
        Some("ci-token"),
        None,
    );
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("local share");
    assert!(!format!("{result:?}").contains("raw-once"));
    let rendered = result_json(result).to_string();
    assert_eq!(rendered.matches(SHARE_URL).count(), 1);
    let requests = fixture.transport.requests();
    assert_eq!(
        requests
            .iter()
            .map(blobyard_api_client::ApiRequest::endpoint)
            .collect::<Vec<_>>(),
        [
            Endpoint::RequestUpload,
            Endpoint::CompleteUpload,
            Endpoint::CreateShare,
        ]
    );
    assert_eq!(
        requests[2].body().expect("share body")["target"],
        "blobyard://team/app/artifact.bin?version=1"
    );
    let storage_requests = storage.await.expect("storage");
    assert!(storage_requests[0].windows(3).any(|bytes| bytes == b"abc"));
}

#[tokio::test]
async fn local_share_rejects_non_files_and_malformed_uris_without_uploading() {
    let root = tempfile::tempdir().expect("root");
    for target in [
        root.path().to_string_lossy().into_owned(),
        root.path()
            .join("missing.bin")
            .to_string_lossy()
            .into_owned(),
        "blobyard://missing/path".to_owned(),
    ] {
        let fixture = Fixture::new(
            &["blobyard", "share", &target],
            Vec::new(),
            Some("ci-token"),
            None,
        );
        let error = fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("invalid local share");
        assert_eq!(error.code(), ErrorCode::InvalidRequest);
        assert!(fixture.transport.requests().is_empty());
    }
}

#[tokio::test]
async fn preview_uploads_an_ignored_filtered_manifest_then_returns_one_capability() {
    let root = tempfile::tempdir().expect("root");
    let site = root.path().join("site");
    std::fs::create_dir_all(site.join("assets")).expect("assets");
    std::fs::create_dir_all(site.join("node_modules")).expect("ignored");
    std::fs::write(site.join("index.html"), b"abc").expect("index");
    std::fs::write(site.join("assets/app.js"), b"abc").expect("asset");
    std::fs::write(site.join("node_modules/skip.js"), b"skip").expect("ignored file");
    let (url, storage) = signed_server(vec![empty_reply("200 OK"), empty_reply("200 OK")]).await;
    let fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "app",
            "preview",
            &site.to_string_lossy(),
            "--expires",
            "24h",
        ],
        vec![
            reservation("single", &url, None, "upload_asset"),
            completion(3, ABC_SHA256),
            reservation("single", &url, None, "upload_index"),
            completion(3, ABC_SHA256),
            preview_response(),
        ],
        Some("ci-token"),
        None,
    );
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("preview");
    assert!(!format!("{result:?}").contains("blobyard.dev"));
    let rendered = result_json(result).to_string();
    assert_eq!(rendered.matches(PREVIEW_URL).count(), 1);
    assert_preview_requests(&fixture);
    assert_eq!(storage.await.expect("storage").len(), 2);
}

#[tokio::test]
async fn preview_requires_a_regular_root_index_before_any_remote_request() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("asset.js"), b"asset").expect("asset");
    let fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "app",
            "preview",
            &root.path().to_string_lossy(),
        ],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    let error = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect_err("missing index");
    assert_eq!(error.code(), ErrorCode::InvalidRequest);
    assert!(error.message().contains("index.html"));
    assert!(fixture.transport.requests().is_empty());
}

fn assert_preview_requests(fixture: &Fixture) {
    let requests = fixture.transport.requests();
    let uploads = requests
        .iter()
        .filter(|request| request.endpoint() == Endpoint::RequestUpload)
        .map(|request| {
            request.body().expect("upload body")["path"]
                .as_str()
                .expect("path")
        })
        .collect::<Vec<_>>();
    assert_eq!(uploads.len(), 2);
    let manifest_id = uploads[0]
        .strip_prefix(".blobyard-preview/")
        .and_then(|path| path.strip_suffix("/assets/app.js"))
        .expect("manifest path");
    assert_eq!(manifest_id.len(), 32);
    assert!(manifest_id.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(
        uploads[1],
        format!(".blobyard-preview/{manifest_id}/index.html")
    );
    let preview = requests.last().expect("preview request");
    assert_eq!(preview.endpoint(), Endpoint::CreatePreview);
    let body = preview.body().expect("preview body");
    assert_eq!(body["manifestId"], manifest_id);
    assert_eq!(body["expires"], "24h");
    assert_eq!(preview.idempotency_key(), None);
}

fn share_response() -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({
            "id": "share_1",
            "shareUrl": SHARE_URL,
            "expiresAt": "2030-01-01T00:00:00Z",
            "notificationStatus": "not_requested"
        }),
        "req_share",
    )
}

fn preview_response() -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({
            "id": "preview_1",
            "previewUrl": PREVIEW_URL,
            "expiresAt": "2030-01-01T00:00:00Z"
        }),
        "req_preview",
    )
}
