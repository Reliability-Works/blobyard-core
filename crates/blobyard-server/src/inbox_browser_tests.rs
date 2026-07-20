#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use crate::{
    contract_test_support::{assert_error, response_json, send},
    transfers::test_seams,
};
use axum::http::StatusCode;
use blobyard_api_client::InboxMetadata;
use http_body_util::BodyExt;

async fn create(fixture: &test_seams::TransferFixture) -> String {
    let body = serde_json::to_vec(&serde_json::json!({
        "workspace": "fixture",
        "project": "project",
        "name": "Customer logs",
        "expires": "1h"
    }))
    .expect("inbox request");
    let created = response_json(send(fixture, "POST", "/v1/inboxes", &body, false).await).await;
    created["data"]["inboxUrl"]
        .as_str()
        .expect("inbox URL")
        .rsplit('/')
        .next()
        .expect("inbox token")
        .to_owned()
}

async fn text(response: Response<Body>) -> String {
    String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes()
            .to_vec(),
    )
    .expect("UTF-8 response")
}

#[tokio::test]
async fn public_inbox_page_is_resolved_redacted_and_locked_down() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let token = create(&fixture).await;
    let response = send(&fixture, "GET", &format!("/i/{token}"), b"", false).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    let policy = response.headers()["content-security-policy"]
        .to_str()
        .expect("CSP");
    assert!(policy.contains("script-src 'self'"));
    assert!(policy.contains("connect-src 'self'"));
    assert!(policy.contains("frame-ancestors 'none'"));
    let html = text(response).await;
    assert!(html.contains("Customer logs"));
    assert!(html.contains("Upload file"));
    assert!(html.contains("/assets/inbox-upload.js"));
    assert!(!html.contains(&token));

    assert_error(
        send(&fixture, "GET", "/i/not-a-capability", b"", false).await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
    assert_error(
        send(&fixture, "GET", "/i/%00", b"", false).await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
    assert_error(
        send(
            &fixture,
            "GET",
            &format!("/i/byin_{}", "a".repeat(64)),
            b"",
            false,
        )
        .await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
}

#[tokio::test]
async fn inbox_browser_script_is_static_nosniff_and_covers_both_transfer_strategies() {
    let fixture = test_seams::fixture(&["fixture"]);
    let response = send(&fixture, "GET", "/assets/inbox-upload.js", b"", false).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        response.headers()["content-type"],
        "application/javascript; charset=utf-8"
    );
    let script = text(response).await;
    for required in [
        "crypto.subtle.digest",
        "x-blobyard-inbox-token",
        "/v1/uploads/parts/request",
        "/v1/uploads/complete",
        "/v1/uploads/abort",
        "url.origin !== location.origin",
    ] {
        assert!(
            script.contains(required),
            "missing browser behavior: {required}"
        );
    }
}

#[test]
fn inbox_page_escapes_public_metadata_and_disables_a_full_inbox() {
    let html = page(&InboxMetadata {
        name: "</h1><script>alert(1)</script>".to_owned(),
        max_files: 20,
        max_bytes: 100,
        expires_at: "2026-07-20T00:00:00Z".to_owned(),
        upload_available: false,
    });
    assert!(html.contains("&lt;/h1&gt;&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!html.contains("<script>alert(1)</script>"));
    assert_eq!(html.matches(" disabled").count(), 2);
}
