#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{contracts, operations};
use crate::{
    auth::Principal,
    contract_test_support::{assert_error, response_json, send},
    error::ApiError,
    test_support::error_status,
    transfers::test_seams,
};
use axum::http::StatusCode;

#[path = "inboxes_tests/management_failures.rs"]
mod management_failures;
#[path = "inboxes_tests/operation_edges.rs"]
mod operation_edges;

fn create_request(name: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "workspace": "fixture",
        "project": "project",
        "name": name,
        "expires": "1h"
    }))
    .expect("inbox request")
}

async fn create(fixture: &test_seams::TransferFixture) -> serde_json::Value {
    let response = send(
        fixture,
        "POST",
        "/v1/inboxes",
        &create_request("Release intake"),
        false,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

fn inbox_token(created: &serde_json::Value) -> &str {
    created["data"]["inboxUrl"]
        .as_str()
        .expect("inbox URL")
        .rsplit('/')
        .next()
        .expect("inbox token")
}

#[tokio::test]
async fn management_and_public_resolve_journey_is_bounded_and_revocable() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let created = create(&fixture).await;
    let token = inbox_token(&created);
    let resolved = response_json(
        send(
            &fixture,
            "GET",
            &format!("/v1/inboxes/resolve?token={token}"),
            b"",
            false,
        )
        .await,
    )
    .await;
    assert_eq!(resolved["data"]["name"], "Release intake");
    assert_eq!(resolved["data"]["maxFiles"], 20);
    assert_eq!(resolved["data"]["maxBytes"], 1_073_741_824_u64);
    assert_eq!(resolved["data"]["uploadAvailable"], true);

    let list_path = "/v1/inboxes?workspace=fixture&project=project";
    let listed = response_json(send(&fixture, "GET", list_path, b"", false).await).await;
    assert_eq!(listed["data"]["items"][0]["name"], "Release intake");
    assert_eq!(listed["data"]["items"][0]["revoked"], false);

    let revoke = serde_json::to_vec(&serde_json::json!({
        "inboxId": created["data"]["id"]
    }))
    .expect("revoke request");
    for _attempt in 0..2 {
        assert_eq!(
            send(&fixture, "POST", "/v1/inboxes/revoke", &revoke, false)
                .await
                .status(),
            StatusCode::OK
        );
    }
    assert_error(
        send(
            &fixture,
            "GET",
            &format!("/v1/inboxes/resolve?token={token}"),
            b"",
            false,
        )
        .await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
    let listed = response_json(send(&fixture, "GET", list_path, b"", false).await).await;
    assert_eq!(listed["data"]["items"][0]["revoked"], true);
}

async fn assert_manager_routes_reject_missing_authority() {
    let fixture = test_seams::fixture(&["fixture"]);
    for (method, path, body) in [
        ("POST", "/v1/inboxes", create_request("Inbox")),
        (
            "GET",
            "/v1/inboxes?workspace=fixture&project=project",
            Vec::new(),
        ),
        (
            "POST",
            "/v1/inboxes/revoke",
            br#"{"inboxId":"missing"}"#.to_vec(),
        ),
    ] {
        assert_error(
            send(&fixture, method, path, &body, false).await,
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        )
        .await;
    }
}

async fn assert_public_resolve_rejects_invalid_tokens() {
    let fixture = test_seams::fixture(&["fixture"]);
    assert_error(
        send(
            &fixture,
            "GET",
            "/v1/inboxes/resolve?token=unknown",
            b"",
            false,
        )
        .await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
    assert_error(
        send(&fixture, "GET", "/v1/inboxes/resolve?token=%ZZ", b"", false).await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
    assert_error(
        send(&fixture, "GET", "/v1/inboxes/resolve?token=%00", b"", false).await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    )
    .await;
}

async fn assert_manager_routes_reject_malformed_inputs() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    assert_error(
        send(
            &fixture,
            "GET",
            "/v1/inboxes?workspace=fixture&project=project&cursor=next",
            b"",
            false,
        )
        .await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
    assert_error(
        send(&fixture, "POST", "/v1/inboxes", b"{", false).await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
    assert_error(
        send(
            &fixture,
            "GET",
            "/v1/inboxes?workspace=%ZZ&project=project",
            b"",
            false,
        )
        .await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
    assert_error(
        send(&fixture, "POST", "/v1/inboxes/revoke", b"{", false).await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
    let mut machine = fixture.principal.clone();
    machine.id = "machine_fixture".to_owned();
    assert_eq!(
        error_status(operations::require_manager(&Principal(machine))),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn routes_reject_missing_authority_machine_callers_and_malformed_inputs() {
    assert_manager_routes_reject_missing_authority().await;
    assert_public_resolve_rejects_invalid_tokens().await;
    assert_manager_routes_reject_malformed_inputs().await;
}

#[tokio::test]
async fn public_resolve_enforces_the_exact_durable_rate_limit() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let created = create(&fixture).await;
    let token = inbox_token(&created);
    let path = format!("/v1/inboxes/resolve?token={token}");
    for _request in 0..contracts::RESOLVE_RATE_LIMIT {
        assert_eq!(
            send(&fixture, "GET", &path, b"", false).await.status(),
            StatusCode::OK
        );
    }
    let limited = send(&fixture, "GET", &path, b"", false).await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(limited.headers().contains_key("retry-after"));
}
