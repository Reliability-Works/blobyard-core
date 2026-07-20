#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::tests::{create_share, upload_object};
use crate::{
    Repository,
    contract_test_support::assert_error,
    repository_fault_tests::{Corruption, FaultingRepository},
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use blobyard_contract::WorkspaceRecord;
use blobyard_core::Slug;
use blobyard_server::transfers::test_seams;
use std::sync::Arc;
use tower::ServiceExt;

async fn send_router(router: Router, method: &str, path: &str, body: &[u8]) -> Response {
    router
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(header::AUTHORIZATION, "Bearer secret")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_vec()))
                .expect("request"),
        )
        .await
        .expect("response")
}

fn faulting_router(fixture: &test_seams::TransferFixture, failure_index: usize) -> Router {
    let mut state = fixture.state.clone();
    let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
    state.repository = Arc::new(FaultingRepository::new(inner, failure_index));
    test_seams::fixture_router(&state)
}

pub(super) async fn seeded_share() -> (test_seams::TransferFixture, String, String) {
    let fixture = test_seams::fixture(&["object:write", "share:manage"]);
    let target = upload_object(&fixture).await;
    let created = create_share(&fixture, &target).await;
    let token = created["data"]["shareUrl"]
        .as_str()
        .expect("share URL")
        .rsplit('/')
        .next()
        .expect("share token")
        .to_owned();
    let id = created["data"]["id"].as_str().expect("share ID").to_owned();
    (fixture, token, id)
}

#[tokio::test]
async fn create_share_propagates_every_repository_failure() {
    for failure_index in 0..7 {
        let fixture = test_seams::fixture(&["object:write", "share:manage"]);
        let target = upload_object(&fixture).await;
        let body = serde_json::to_vec(&serde_json::json!({
            "target": target,
            "expires": "1h",
            "notify": null
        }))
        .expect("request");
        assert_error(
            send_router(
                faulting_router(&fixture, failure_index),
                "POST",
                "/v1/shares",
                &body,
            )
            .await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
        )
        .await;
    }
}

#[tokio::test]
async fn create_share_validates_the_complete_request_before_persistence() {
    let fixture = test_seams::fixture(&["object:write", "share:manage"]);
    let target = upload_object(&fixture).await;
    for body in [
        b"{".as_slice(),
        br#"{"target":"invalid","expires":"1h","notify":null}"#.as_slice(),
    ] {
        assert_error(
            send_router(fixture.router(), "POST", "/v1/shares", body).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
    for body in [
        serde_json::to_vec(&serde_json::json!({
            "target": target,
            "expires": "31d",
            "notify": null
        }))
        .expect("expiry request"),
        serde_json::to_vec(&serde_json::json!({
            "target": target,
            "expires": "1h",
            "notify": "invalid"
        }))
        .expect("notification request"),
    ] {
        assert_error(
            send_router(fixture.router(), "POST", "/v1/shares", &body).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
    assert!(
        fixture
            .state
            .repository
            .list_shares(&fixture.principal.workspace_id)
            .expect("shares")
            .is_empty()
    );
}

#[tokio::test]
async fn list_shares_rejects_foreign_queries_and_repository_failures() {
    let fixture = test_seams::fixture(&["share:manage"]);
    fixture
        .state
        .repository
        .create_workspace(&WorkspaceRecord {
            id: "workspace_foreign".to_owned(),
            name: "Foreign".to_owned(),
            slug: Slug::new("foreign").expect("slug"),
        })
        .expect("foreign workspace");
    assert_error(
        send_router(fixture.router(), "GET", "/v1/shares?workspace=foreign", b"").await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
    assert_error(
        send_router(
            fixture.router(),
            "GET",
            "/v1/shares?workspace=fixture&workspace=foreign",
            b"",
        )
        .await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
    for failure_index in 0..3 {
        assert_error(
            send_router(
                faulting_router(&fixture, failure_index),
                "GET",
                "/v1/shares?workspace=fixture",
                b"",
            )
            .await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
        )
        .await;
    }
}

#[tokio::test]
async fn public_share_routes_conceal_input_and_provider_failures() {
    let (fixture, token, _id) = seeded_share().await;
    for path in ["/v1/shares/resolve", "/v1/shares/resolve?token="] {
        assert_error(
            send_router(fixture.router(), "GET", path, b"").await,
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
        )
        .await;
    }
    assert_error(
        send_router(
            faulting_router(&fixture, 0),
            "GET",
            &format!("/v1/shares/resolve?token={token}"),
            b"",
        )
        .await,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    )
    .await;
    for failure_index in 0..2 {
        let body = serde_json::to_vec(&serde_json::json!({ "token": token })).expect("request");
        assert_error(
            send_router(
                faulting_router(&fixture, failure_index),
                "POST",
                "/v1/shares/download",
                &body,
            )
            .await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
        )
        .await;
    }
    for body in [
        br#"{"token":""}"#.as_slice(),
        br#"{"token":"missing"}"#.as_slice(),
    ] {
        assert_error(
            send_router(fixture.router(), "POST", "/v1/shares/download", body).await,
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
        )
        .await;
    }
}

#[tokio::test]
async fn share_responses_conceal_incomplete_object_metadata() {
    let (fixture, token, _id) = seeded_share().await;
    let mut state = fixture.state.clone();
    let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
    state.repository = Arc::new(FaultingRepository::corrupting(
        inner,
        Corruption::ShareObjectSize,
    ));
    let router = test_seams::fixture_router(&state);
    for path in [
        format!("/v1/shares/resolve?token={token}"),
        format!("/s/{token}"),
    ] {
        assert_error(
            send_router(router.clone(), "GET", &path, b"").await,
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
        )
        .await;
    }
}

#[tokio::test]
async fn revoke_share_validates_input_and_propagates_repository_failures() {
    let (fixture, _token, id) = seeded_share().await;
    assert_error(
        send_router(fixture.router(), "POST", "/v1/shares/revoke", b"{").await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
    let missing = br#"{"shareId":"missing"}"#;
    assert_error(
        send_router(fixture.router(), "POST", "/v1/shares/revoke", missing).await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
    let body = serde_json::to_vec(&serde_json::json!({ "shareId": id })).expect("request");
    for failure_index in 0..2 {
        assert_error(
            send_router(
                faulting_router(&fixture, failure_index),
                "POST",
                "/v1/shares/revoke",
                &body,
            )
            .await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
        )
        .await;
    }
}
