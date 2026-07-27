#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::{
    contract_test_support::{assert_error, send},
    transfers::test_seams,
};
use axum::http::StatusCode;

#[tokio::test]
async fn identity_routes_map_extractor_rejections_to_the_public_error_contract() {
    let fixture = test_seams::fixture(&["yard:manage"]);
    for path in ["/v1/yards/management-roles", "/v1/yards/application-policy"] {
        assert_error(
            send(&fixture, "GET", path, b"", false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
    for path in [
        "/v1/yards/management-roles/set",
        "/v1/yards/management-roles/revoke",
        "/v1/yards/application-policy",
        "/v1/yards/access/roles",
    ] {
        assert_error(
            send(&fixture, "POST", path, b"{", false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
}

#[tokio::test]
async fn identity_routes_require_yard_management_scope() {
    let fixture = test_seams::fixture(&["yard:read"]);
    for (method, path, body) in [
        (
            "GET",
            "/v1/yards/management-roles?yardId=yard_fixture",
            serde_json::json!(null),
        ),
        (
            "POST",
            "/v1/yards/management-roles/set",
            serde_json::json!({
                "yardId": "yard_fixture",
                "userId": "user_fixture",
                "role": "admin",
            }),
        ),
        (
            "POST",
            "/v1/yards/management-roles/revoke",
            serde_json::json!({
                "yardId": "yard_fixture",
                "userId": "user_fixture",
            }),
        ),
        (
            "GET",
            "/v1/yards/application-policy?yardId=yard_fixture",
            serde_json::json!(null),
        ),
        (
            "POST",
            "/v1/yards/application-policy",
            serde_json::json!({
                "yardId": "yard_fixture",
                "sourceManifestDigest": "a".repeat(64),
                "defaultRole": null,
                "roles": {},
            }),
        ),
        (
            "POST",
            "/v1/yards/access/roles",
            serde_json::json!({
                "yardId": "yard_fixture",
                "grantId": "yardgrant_fixture",
                "appRoles": [],
            }),
        ),
    ] {
        let encoded = body.to_string();
        assert_error(
            send(&fixture, method, path, encoded.as_bytes(), false).await,
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        )
        .await;
    }
}

#[tokio::test]
async fn application_policy_route_rejects_an_invalid_role_graph() {
    let (fixture, _principal, yard_id) = super::access_edge_tests::manager_fixture();
    let body = serde_json::json!({
        "yardId": yard_id,
        "sourceManifestDigest": "a".repeat(64),
        "defaultRole": "missing",
        "roles": {},
    })
    .to_string();
    assert_error(
        send(
            &fixture,
            "POST",
            "/v1/yards/application-policy",
            body.as_bytes(),
            false,
        )
        .await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
}
