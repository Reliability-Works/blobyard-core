#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::{
    contract_test_support::{assert_error, response_json, send},
    transfers::test_seams,
};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

const MANIFEST_ID: &str = "1234567890abcdef1234567890abcdef";

#[path = "previews_tests/contracts.rs"]
mod contract_tests;
#[path = "previews_tests/operations.rs"]
mod operation_tests;
#[path = "previews_tests/public_failures.rs"]
mod public_failure_tests;
#[path = "previews_tests/upload_fixture.rs"]
mod upload_fixture;
pub use upload_fixture::upload;

async fn create(fixture: &test_seams::TransferFixture) -> serde_json::Value {
    let body = serde_json::to_vec(&serde_json::json!({
        "workspace": "fixture",
        "project": "project",
        "manifestId": MANIFEST_ID,
        "expires": "1h"
    }))
    .expect("preview request");
    let response = send(fixture, "POST", "/v1/previews", &body, false).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

fn preview_capability(created: &serde_json::Value) -> String {
    url::Url::parse(created["data"]["previewUrl"].as_str().expect("preview URL"))
        .expect("parsed preview URL")
        .host_str()
        .expect("preview host")
        .strip_suffix(".localhost")
        .expect("isolated preview host")
        .to_owned()
}

async fn public_request(
    fixture: &test_seams::TransferFixture,
    method: &str,
    path: &str,
    capability: &str,
) -> axum::response::Response {
    fixture
        .router()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(header::HOST, format!("{capability}.localhost:8787"))
                .body(Body::empty())
                .expect("public request"),
        )
        .await
        .expect("public response")
}

async fn assert_public_preview_page(fixture: &test_seams::TransferFixture, capability: &str) {
    let page = public_request(fixture, "GET", "/", capability).await;
    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(page.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        page.headers()[header::CONTENT_TYPE],
        "text/html; charset=utf-8"
    );
    assert_eq!(page.headers()[header::REFERRER_POLICY], "no-referrer");
    assert_eq!(
        page.into_body()
            .collect()
            .await
            .expect("preview body")
            .to_bytes()
            .as_ref(),
        b"<h1>Preview</h1>"
    );
}

async fn assert_public_preview_asset(fixture: &test_seams::TransferFixture, capability: &str) {
    let asset = public_request(fixture, "GET", "/assets/app.js", capability).await;
    assert_eq!(asset.status(), StatusCode::OK);
    assert_eq!(
        asset.headers()[header::CONTENT_TYPE],
        "text/javascript; charset=utf-8"
    );
    let head = public_request(fixture, "HEAD", "/assets/app.js", capability).await;
    assert_eq!(head.status(), StatusCode::OK);
    assert!(
        head.into_body()
            .collect()
            .await
            .expect("HEAD body")
            .to_bytes()
            .is_empty()
    );
}

async fn revoke_and_assert_hidden(
    fixture: &test_seams::TransferFixture,
    created: &serde_json::Value,
    capability: &str,
) {
    let revoke = serde_json::to_vec(&serde_json::json!({
        "previewId": created["data"]["id"]
    }))
    .expect("revoke request");
    for _attempt in 0..2 {
        assert_eq!(
            send(fixture, "POST", "/v1/previews/revoke", &revoke, false)
                .await
                .status(),
            StatusCode::OK
        );
    }
    assert_eq!(
        public_request(fixture, "GET", "/", capability)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn preview_journey_snapshots_serves_lists_and_revokes_the_manifest() {
    let fixture = test_seams::fixture(&["object:write", "share:manage"]);
    let root = format!(".blobyard-preview/{MANIFEST_ID}");
    upload(
        &fixture,
        &format!("{root}/index.html"),
        "text/html; charset=utf-8",
        b"<h1>Preview</h1>",
    )
    .await;
    upload(
        &fixture,
        &format!("{root}/assets/app.js"),
        "text/javascript; charset=utf-8",
        b"console.log('preview');",
    )
    .await;

    let created = create(&fixture).await;
    let capability = preview_capability(&created);
    let listed = response_json(
        send(
            &fixture,
            "GET",
            "/v1/previews?workspace=fixture&project=project",
            b"",
            false,
        )
        .await,
    )
    .await;
    assert_eq!(listed["data"]["items"][0]["status"], "active");
    assert_eq!(listed["data"]["items"][0]["id"], created["data"]["id"]);

    assert_public_preview_page(&fixture, &capability).await;
    assert_public_preview_asset(&fixture, &capability).await;
    revoke_and_assert_hidden(&fixture, &created, &capability).await;
}

#[tokio::test]
async fn preview_management_routes_fail_closed() {
    let unauthorized = test_seams::fixture(&["object:write"]);
    for (method, path, body) in [
        (
            "POST",
            "/v1/previews",
            format!(
                "{{\"workspace\":\"fixture\",\"project\":\"project\",\"manifestId\":\"{MANIFEST_ID}\",\"expires\":null}}"
            )
            .into_bytes(),
        ),
        (
            "GET",
            "/v1/previews?workspace=fixture&project=project",
            Vec::new(),
        ),
        (
            "POST",
            "/v1/previews/revoke",
            br#"{"previewId":"missing"}"#.to_vec(),
        ),
    ] {
        assert_error(
            send(&unauthorized, method, path, &body, false).await,
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        )
        .await;
    }
    let authorized = test_seams::fixture(&["share:manage"]);
    for (method, path, body) in [
        ("POST", "/v1/previews", b"{".as_slice()),
        ("GET", "/v1/previews?workspace=fixture", b"".as_slice()),
        ("POST", "/v1/previews/revoke", b"{".as_slice()),
    ] {
        assert_error(
            send(&authorized, method, path, body, false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
    assert_error(
        send(
            &authorized,
            "POST",
            "/v1/previews",
            br#"{"workspace":"fixture","project":"project","manifestId":"short","expires":null}"#,
            false,
        )
        .await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
}

#[tokio::test]
async fn public_preview_host_and_path_fail_closed() {
    let authorized = test_seams::fixture(&["share:manage"]);
    assert_eq!(
        public_request(&authorized, "GET", "/", &"a".repeat(52))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        public_request(&authorized, "GET", "/", &format!("{}b", "a".repeat(51)),)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        public_request(&authorized, "GET", "/%2e%2e/secret", &"a".repeat(52))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        public_request(&authorized, "POST", "/", &"a".repeat(52))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}
