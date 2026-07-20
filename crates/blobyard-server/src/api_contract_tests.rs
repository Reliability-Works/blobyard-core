#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::contract_test_support::{assert_error, response_json, send};
use axum::http::StatusCode;
use blobyard_server::transfers::test_seams;

#[tokio::test]
async fn bootstrap_and_namespace_bodies_fail_before_durable_mutation() {
    let fixture = test_seams::fixture(&["project:write"]);
    let counts = fixture.namespace_counts();
    for (path, body) in [
        ("/v1/bootstrap/exchange", b"{".as_slice()),
        (
            "/v1/bootstrap/exchange",
            br#"{"name":"","token":"bootstrap"}"#.as_slice(),
        ),
        (
            "/v1/bootstrap/exchange",
            br#"{"name":"Valid","token":"bootstrap"}"#.as_slice(),
        ),
        ("/v1/workspaces", b"{".as_slice()),
        ("/v1/workspaces", br#"{"name":""}"#.as_slice()),
        ("/v1/workspaces", br#"{"name":"---"}"#.as_slice()),
        ("/v1/workspaces/rename", b"{".as_slice()),
        (
            "/v1/workspaces/rename",
            br#"{"workspace":"fixture","name":"---"}"#.as_slice(),
        ),
        (
            "/v1/workspaces/rename",
            br#"{"workspace":"fixture","name":"Valid","extra":true}"#.as_slice(),
        ),
        ("/v1/projects", b"{".as_slice()),
        (
            "/v1/projects",
            br#"{"workspace":"fixture","name":""}"#.as_slice(),
        ),
        (
            "/v1/projects",
            br#"{"workspace":"Invalid Slug","name":"Valid"}"#.as_slice(),
        ),
        (
            "/v1/projects",
            br#"{"workspace":"fixture","name":"---"}"#.as_slice(),
        ),
    ] {
        assert_error(
            send(&fixture, "POST", path, body, false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
        assert_eq!(fixture.namespace_counts(), counts);
    }
}

#[tokio::test]
async fn namespace_mutations_require_their_exact_scopes_before_writes() {
    let fixture = test_seams::fixture(&["fixture"]);
    let counts = fixture.namespace_counts();
    for (path, body) in [
        ("/v1/workspaces", br#"{"name":"Valid"}"#.as_slice()),
        (
            "/v1/projects",
            br#"{"workspace":"fixture","name":"Valid"}"#.as_slice(),
        ),
        (
            "/v1/workspaces/rename",
            br#"{"workspace":"fixture","name":"Valid"}"#.as_slice(),
        ),
    ] {
        assert_error(
            send(&fixture, "POST", path, body, false).await,
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        )
        .await;
        assert_eq!(fixture.namespace_counts(), counts);
    }
}

#[tokio::test]
async fn workspace_rename_updates_followup_identity_and_namespace_reads() {
    let fixture = test_seams::fixture(&["project:write", "workspace:read"]);
    let renamed = response_json(
        send(
            &fixture,
            "POST",
            "/v1/workspaces/rename",
            br#"{"workspace":"fixture","name":"Release Engineering"}"#,
            false,
        )
        .await,
    )
    .await;
    assert_eq!(renamed["data"]["slug"], "release-engineering");

    let identity = response_json(send(&fixture, "GET", "/v1/cli/whoami", b"", false).await).await;
    assert_eq!(
        identity["data"]["defaultWorkspace"]["slug"],
        "release-engineering"
    );
    let workspaces = response_json(send(&fixture, "GET", "/v1/workspaces", b"", false).await).await;
    assert_eq!(
        workspaces["data"]["items"][0]["slug"],
        "release-engineering"
    );
}

#[tokio::test]
async fn namespace_lists_reject_unsupported_or_malformed_queries() {
    let fixture = test_seams::fixture(&["project:read", "workspace:read"]);
    for path in [
        "/v1/workspaces?cursor=next",
        "/v1/workspaces?cursor=%FF",
        "/v1/workspaces?cursor=first&cursor=second",
        "/v1/projects",
        "/v1/projects?workspace=fixture&cursor=next",
        "/v1/projects?workspace=Invalid%20Slug",
    ] {
        assert_error(
            send(&fixture, "GET", path, b"", false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
}

#[tokio::test]
async fn namespace_lists_require_their_exact_read_scopes() {
    let fixture = test_seams::fixture(&["fixture"]);
    for path in ["/v1/workspaces", "/v1/projects?workspace=fixture"] {
        assert_error(
            send(&fixture, "GET", path, b"", false).await,
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        )
        .await;
    }
}

#[tokio::test]
async fn cli_session_routes_enforce_scope_shape_and_backing_credential_revocation() {
    let forbidden = test_seams::fixture(&["fixture"]);
    for (method, path, body) in [
        ("GET", "/v1/cli/sessions", b"".as_slice()),
        (
            "POST",
            "/v1/cli/sessions/revoke",
            br#"{"sessionId":"session_fixture"}"#.as_slice(),
        ),
    ] {
        assert_error(
            send(&forbidden, method, path, body, false).await,
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        )
        .await;
    }

    for body in [
        b"{".as_slice(),
        br#"{"sessionId":"session_fixture","extra":true}"#.as_slice(),
        br#"{"session_id":"session_fixture"}"#.as_slice(),
    ] {
        let fixture = test_seams::fixture(&["sessions:manage"]);
        assert_error(
            send(&fixture, "POST", "/v1/cli/sessions/revoke", body, false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }

    let fixture = test_seams::fixture(&["sessions:manage"]);
    let sessions = response_json(send(&fixture, "GET", "/v1/cli/sessions", b"", false).await).await;
    assert_eq!(sessions["data"][0]["id"], "session_fixture");
    assert_eq!(sessions["data"][0]["platform"], "test");
    assert!(sessions["data"][0]["lastUsedAt"].is_number());
    assert!(!sessions.to_string().contains("secretHash"));

    let revoked = response_json(
        send(
            &fixture,
            "POST",
            "/v1/cli/sessions/revoke",
            br#"{"sessionId":"session_fixture"}"#,
            false,
        )
        .await,
    )
    .await;
    assert_eq!(revoked["data"]["status"], "revoked");
    assert_error(
        send(&fixture, "GET", "/v1/cli/whoami", b"", false).await,
        StatusCode::UNAUTHORIZED,
        "INVALID_TOKEN",
    )
    .await;
}
