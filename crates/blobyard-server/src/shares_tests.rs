#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::contract_test_support::{assert_error, response_json, send};
use axum::http::{StatusCode, header};
use blobyard_server::transfers::test_seams;
use http_body_util::BodyExt;

const CHECKSUM: &str = "2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881";

pub(super) async fn upload_object(fixture: &test_seams::TransferFixture) -> String {
    let request = serde_json::to_vec(&serde_json::json!({
        "workspace": "fixture",
        "project": "project",
        "path": "shared/example.txt",
        "filename": "example.txt",
        "sizeBytes": 1,
        "checksumSha256": CHECKSUM,
        "contentType": "text/plain"
    }))
    .expect("upload request");
    let reserved =
        response_json(send(fixture, "POST", "/v1/uploads/request", &request, true).await).await;
    let upload_url = reserved["data"]["uploadUrl"].as_str().expect("upload URL");
    let upload_path = upload_url
        .strip_prefix("http://127.0.0.1:8787")
        .expect("upload path");
    assert_eq!(
        send(fixture, "PUT", upload_path, b"x", false)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    let upload_id = reserved["data"]["uploadId"].as_str().expect("upload ID");
    let complete = serde_json::to_vec(&serde_json::json!({
        "uploadId": upload_id,
        "parts": []
    }))
    .expect("complete request");
    let completed =
        response_json(send(fixture, "POST", "/v1/uploads/complete", &complete, false).await).await;
    completed["data"]["uri"]
        .as_str()
        .expect("object URI")
        .to_owned()
}

pub(super) async fn create_share(
    fixture: &test_seams::TransferFixture,
    target: &str,
) -> serde_json::Value {
    let request = serde_json::to_vec(&serde_json::json!({
        "target": target,
        "expires": "1h",
        "notify": null
    }))
    .expect("share request");
    let response = send(fixture, "POST", "/v1/shares", &request, false).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

async fn assert_share_resolves(fixture: &test_seams::TransferFixture, token: &str) {
    let resolved = response_json(
        send(
            fixture,
            "GET",
            &format!("/v1/shares/resolve?token={token}"),
            b"",
            false,
        )
        .await,
    )
    .await;
    assert_eq!(resolved["data"]["filename"], "example.txt");
    assert_eq!(resolved["data"]["contentTypeClass"], "text");
    assert_eq!(resolved["data"]["downloadAvailable"], true);
}

async fn assert_browser_download(fixture: &test_seams::TransferFixture, token: &str) {
    let page = send(fixture, "GET", &format!("/s/{token}"), b"", false).await;
    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(
        page.headers()
            .get(header::CACHE_CONTROL)
            .expect("cache control"),
        "no-store"
    );
    assert_eq!(
        page.headers()
            .get(header::REFERRER_POLICY)
            .expect("referrer policy"),
        "no-referrer"
    );
    let html = page
        .into_body()
        .collect()
        .await
        .expect("share page")
        .to_bytes();
    let html = std::str::from_utf8(&html).expect("share HTML");
    assert!(html.contains("example.txt"));
    assert!(html.contains(&format!("action=\"/s/{token}/download\"")));

    let redirect = send(fixture, "POST", &format!("/s/{token}/download"), b"", false).await;
    assert_eq!(redirect.status(), StatusCode::SEE_OTHER);
    let path = redirect
        .headers()
        .get(header::LOCATION)
        .expect("download redirect")
        .to_str()
        .expect("download location")
        .strip_prefix("http://127.0.0.1:8787")
        .expect("download path");
    assert_download_bytes(fixture, path, "browser download body").await;
}

async fn assert_download_bytes(fixture: &test_seams::TransferFixture, path: &str, context: &str) {
    let bytes = send(fixture, "GET", path, b"", false)
        .await
        .into_body()
        .collect()
        .await
        .expect(context)
        .to_bytes();
    assert_eq!(bytes.as_ref(), b"x");
}

async fn assert_api_download(fixture: &test_seams::TransferFixture, token: &str) {
    let request =
        serde_json::to_vec(&serde_json::json!({ "token": token })).expect("download request");
    let issued =
        response_json(send(fixture, "POST", "/v1/shares/download", &request, false).await).await;
    let path = issued["data"]["downloadUrl"]
        .as_str()
        .expect("download URL")
        .strip_prefix("http://127.0.0.1:8787")
        .expect("download path");
    assert_download_bytes(fixture, path, "download body").await;
}

async fn revoke_and_assert_concealed(
    fixture: &test_seams::TransferFixture,
    token: &str,
    share_id: &serde_json::Value,
) {
    let listed =
        response_json(send(fixture, "GET", "/v1/shares?workspace=fixture", b"", false).await).await;
    assert_eq!(listed["data"]["items"][0]["consumedCount"], 2);
    assert_eq!(listed["data"]["items"][0]["status"], "active");

    let request =
        serde_json::to_vec(&serde_json::json!({ "shareId": share_id })).expect("revoke request");
    for _ in 0..2 {
        assert_eq!(
            send(fixture, "POST", "/v1/shares/revoke", &request, false)
                .await
                .status(),
            StatusCode::OK
        );
    }
    assert_error(
        send(
            fixture,
            "GET",
            &format!("/v1/shares/resolve?token={token}"),
            b"",
            false,
        )
        .await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
    assert_error(
        send(fixture, "GET", &format!("/s/{token}"), b"", false).await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
}

#[tokio::test]
async fn share_journey_preserves_the_object_and_revokes_only_the_capability() {
    let fixture = test_seams::fixture(&["object:write", "share:manage"]);
    let target = upload_object(&fixture).await;
    let created = create_share(&fixture, &target).await;
    assert_eq!(created["data"]["notificationStatus"], "not_requested");
    let token = created["data"]["shareUrl"]
        .as_str()
        .expect("share URL")
        .rsplit('/')
        .next()
        .expect("share token");
    assert_share_resolves(&fixture, token).await;
    assert_browser_download(&fixture, token).await;
    assert_api_download(&fixture, token).await;
    revoke_and_assert_concealed(&fixture, token, &created["data"]["id"]).await;
}

#[tokio::test]
async fn share_routes_fail_closed_for_missing_authority_and_malformed_inputs() {
    let fixture = test_seams::fixture(&["fixture"]);
    for (method, path, body, status) in [
        (
            "POST",
            "/v1/shares",
            br#"{"target":"blobyard://fixture/project/file","expires":null,"notify":null}"#
                .as_slice(),
            StatusCode::FORBIDDEN,
        ),
        (
            "GET",
            "/v1/shares?workspace=fixture",
            b"".as_slice(),
            StatusCode::FORBIDDEN,
        ),
        (
            "POST",
            "/v1/shares/revoke",
            br#"{"shareId":"missing"}"#.as_slice(),
            StatusCode::FORBIDDEN,
        ),
    ] {
        assert_error(
            send(&fixture, method, path, body, false).await,
            status,
            "FORBIDDEN",
        )
        .await;
    }
    assert_error(
        send(
            &fixture,
            "GET",
            "/v1/shares/resolve?token=unknown",
            b"",
            false,
        )
        .await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
    assert_error(
        send(&fixture, "POST", "/v1/shares/download", b"{", false).await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
}
