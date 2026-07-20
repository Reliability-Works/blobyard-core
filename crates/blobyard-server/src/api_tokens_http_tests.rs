#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::{
    contract_test_support::{assert_error, response_json, send},
    repository_fault_tests::FaultingRepository,
    transfers::test_seams::{self, TransferFixture},
};
use axum::http::StatusCode;
use std::sync::Arc;

#[path = "api_tokens_http_tests/support.rs"]
mod support;

use support::{item, send_as};

#[tokio::test]
async fn token_routes_create_list_authenticate_revoke_and_audit_without_secret_disclosure() {
    let fixture = test_seams::fixture(&["audit:read", "object:read", "tokens:manage"]);
    let (raw_token, token_id) = create_token(&fixture).await;
    assert_redacted_list(&fixture, &token_id, &raw_token).await;
    authenticate_and_mark_used(&fixture, &token_id, &raw_token).await;
    revoke_and_verify_audit(&fixture, &token_id, &raw_token).await;
}

async fn create_token(fixture: &TransferFixture) -> (String, String) {
    let created = send(
        fixture,
        "POST",
        "/v1/api-tokens",
        br#"{"expiresInDays":7,"name":"Build agent","scopes":["object:read"]}"#,
        false,
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let created = response_json(created).await;
    let raw_token = created["data"]["rawToken"]
        .as_str()
        .expect("raw token")
        .to_owned();
    let token_id = created["data"]["id"].as_str().expect("token id").to_owned();
    assert!(raw_token.starts_with("byd_pat_"));
    assert_eq!(created["data"]["name"], "Build agent");
    assert_eq!(
        created["data"]["scopes"],
        serde_json::json!(["object:read"])
    );
    assert!(created["data"].get("workspaceId").is_none());
    assert!(created["data"].get("projectId").is_none());
    (raw_token, token_id)
}

async fn assert_redacted_list(fixture: &TransferFixture, token_id: &str, raw_token: &str) {
    let listed = response_json(send(fixture, "GET", "/v1/api-tokens", b"", false).await).await;
    let summary = item(&listed, token_id);
    assert_eq!(summary["name"], "Build agent");
    assert_eq!(summary["status"], "active");
    assert_eq!(summary["lastUsedAt"], serde_json::Value::Null);
    assert_eq!(
        summary["tokenPrefix"],
        raw_token.chars().take(16).collect::<String>()
    );
    assert!(summary.get("workspaceId").is_none());
    assert!(summary.get("projectId").is_none());
    let listed_text = listed.to_string();
    assert!(!listed_text.contains(raw_token));
    assert!(!listed_text.contains("rawToken"));
    assert!(!listed_text.contains("secretHash"));
}

async fn authenticate_and_mark_used(fixture: &TransferFixture, token_id: &str, raw_token: &str) {
    let identity = send_as(fixture.router(), raw_token, "GET", "/v1/cli/whoami", b"").await;
    assert_eq!(identity.status(), StatusCode::OK);
    assert_eq!(
        response_json(identity).await["data"]["principalId"],
        token_id
    );
    let used = response_json(send(fixture, "GET", "/v1/api-tokens", b"", false).await).await;
    assert!(item(&used, token_id)["lastUsedAt"].is_number());
}

async fn revoke_and_verify_audit(fixture: &TransferFixture, token_id: &str, raw_token: &str) {
    let revoked = response_json(
        send(
            fixture,
            "POST",
            "/v1/api-tokens/revoke",
            format!(r#"{{"tokenId":"{token_id}"}}"#).as_bytes(),
            false,
        )
        .await,
    )
    .await;
    assert_eq!(revoked["data"]["status"], "revoked");
    assert_error(
        send_as(fixture.router(), raw_token, "GET", "/v1/cli/whoami", b"").await,
        StatusCode::UNAUTHORIZED,
        "INVALID_TOKEN",
    )
    .await;
    assert_revoke_outcomes(fixture, token_id).await;
    let listed = response_json(send(fixture, "GET", "/v1/api-tokens", b"", false).await).await;
    assert_eq!(item(&listed, token_id)["status"], "revoked");
    assert_token_audit(fixture);
}

async fn assert_revoke_outcomes(fixture: &TransferFixture, token_id: &str) {
    for (id, status) in [(token_id, "already_revoked"), ("token_missing", "invalid")] {
        let response = response_json(
            send(
                fixture,
                "POST",
                "/v1/api-tokens/revoke",
                format!(r#"{{"tokenId":"{id}"}}"#).as_bytes(),
                false,
            )
            .await,
        )
        .await;
        assert_eq!(response["data"]["status"], status);
    }
}

fn assert_token_audit(fixture: &TransferFixture) {
    let audit = fixture
        .state
        .repository
        .list_audit(&fixture.principal.workspace_id, None, 10)
        .expect("audit");
    assert_eq!(audit.items.len(), 2);
    assert_eq!(audit.items[0].action, "api_token.revoked");
    assert_eq!(audit.items[1].action, "api_token.created");
}

#[tokio::test]
async fn cleanup_tokens_require_and_return_the_exact_project_binding() {
    let fixture = test_seams::fixture(&["object:write", "tokens:manage"]);
    let response = send(
        &fixture,
        "POST",
        "/v1/api-tokens",
        br#"{"expiresInDays":30,"name":"Cleanup agent","project":"project","scopes":["object:write"],"workspace":"fixture"}"#,
        false,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let value = response_json(response).await;
    assert_eq!(value["data"]["workspaceId"], "workspace_fixture");
    assert_eq!(value["data"]["projectId"], "project_fixture");
    let token_id = value["data"]["id"].as_str().expect("token id");
    let listed = response_json(send(&fixture, "GET", "/v1/api-tokens", b"", false).await).await;
    assert_eq!(item(&listed, token_id)["workspaceId"], "workspace_fixture");
    assert_eq!(item(&listed, token_id)["projectId"], "project_fixture");
}

#[tokio::test]
async fn token_routes_reject_invalid_bodies_scope_escalation_and_missing_management_authority() {
    let fixture = test_seams::fixture(&["object:read", "object:write", "tokens:manage"]);
    assert_invalid_create_requests(&fixture).await;
    assert_error(
        send(&fixture, "POST", "/v1/api-tokens/revoke", b"{", false).await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
    assert_unprivileged_token_routes().await;
}

async fn assert_invalid_create_requests(fixture: &TransferFixture) {
    let before = fixture
        .state
        .repository
        .list_api_tokens()
        .expect("tokens")
        .len();
    for body in [
        b"{".as_slice(),
        br#"{"expiresInDays":7,"name":"Valid name","scopes":["object:read"],"unknown":true}"#,
        br#"{"expiresInDays":1,"name":"Valid name","scopes":["object:read"]}"#,
        br#"{"expiresInDays":7,"name":"x","scopes":["object:read"]}"#,
        br#"{"expiresInDays":7,"name":"line\nbreak","scopes":["object:read"]}"#,
        br#"{"expiresInDays":7,"name":"Valid name","scopes":[]}"#,
        br#"{"expiresInDays":7,"name":"Valid name","scopes":["unknown"]}"#,
        br#"{"expiresInDays":7,"name":"Valid name","project":"project","scopes":["object:read"],"workspace":"fixture"}"#,
        br#"{"expiresInDays":7,"name":"Valid name","scopes":["object:write"]}"#,
    ] {
        assert_error(
            send(fixture, "POST", "/v1/api-tokens", body, false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
    assert_error(
        send(
            fixture,
            "POST",
            "/v1/api-tokens",
            br#"{"expiresInDays":7,"name":"Escalation","scopes":["audit:read"]}"#,
            false,
        )
        .await,
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
    )
    .await;
    assert_eq!(
        fixture
            .state
            .repository
            .list_api_tokens()
            .expect("tokens")
            .len(),
        before
    );
    assert!(
        fixture
            .state
            .repository
            .list_audit(&fixture.principal.workspace_id, None, 10)
            .expect("audit")
            .items
            .is_empty()
    );
}

async fn assert_unprivileged_token_routes() {
    let unprivileged = test_seams::fixture(&["object:read"]);
    for (method, path, body) in [
        ("GET", "/v1/api-tokens", b"".as_slice()),
        (
            "POST",
            "/v1/api-tokens",
            br#"{"expiresInDays":7,"name":"Valid name","scopes":["object:read"]}"#,
        ),
        (
            "POST",
            "/v1/api-tokens/revoke",
            br#"{"tokenId":"token_fixture"}"#,
        ),
    ] {
        assert_error(
            send(&unprivileged, method, path, body, false).await,
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        )
        .await;
    }
}

#[tokio::test]
async fn token_routes_map_repository_failures_without_partial_mutation() {
    for (method, path, body) in [
        (
            "POST",
            "/v1/api-tokens",
            br#"{"expiresInDays":7,"name":"Build agent","scopes":["object:read"]}"#.as_slice(),
        ),
        ("GET", "/v1/api-tokens", b"".as_slice()),
        (
            "POST",
            "/v1/api-tokens/revoke",
            br#"{"tokenId":"token_fixture"}"#.as_slice(),
        ),
    ] {
        let fixture = test_seams::fixture(&["object:read", "tokens:manage"]);
        let mut state = fixture.state.clone();
        state.repository = Arc::new(FaultingRepository::new(Arc::clone(&state.repository), 1));
        assert_error(
            send_as(
                test_seams::fixture_router(&state),
                "secret",
                method,
                path,
                body,
            )
            .await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
        )
        .await;
        assert_eq!(
            fixture
                .state
                .repository
                .list_api_tokens()
                .expect("tokens")
                .len(),
            0
        );
        assert!(
            fixture
                .state
                .repository
                .list_audit(&fixture.principal.workspace_id, None, 10)
                .expect("audit")
                .items
                .is_empty()
        );
    }
}
