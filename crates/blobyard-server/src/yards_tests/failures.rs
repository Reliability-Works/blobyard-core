use super::{public_request, start};
use crate::{
    contract_test_support::{assert_error, send},
    transfers::test_seams,
};
use axum::http::StatusCode;

const DENIED_ROUTES: [(&str, &str, &[u8]); 7] = [
    (
        "GET",
        "/v1/yards?workspace=fixture&project=project",
        b"",
    ),
    ("GET", "/v1/yards/deploys?yardId=missing", b""),
    (
        "POST",
        "/v1/yards/deploys/start",
        br#"{"workspace":"fixture","project":"project","name":"site","clientDeployId":"client-deploy-0001","spa":false,"cleanUrls":false,"public":true}"#,
    ),
    (
        "POST",
        "/v1/yards/deploys/finalise",
        br#"{"deployId":"missing"}"#,
    ),
    (
        "POST",
        "/v1/yards/deploys/fail",
        br#"{"deployId":"missing","failureCode":"UPLOAD_FAILED","failureMessage":"failed"}"#,
    ),
    ("POST", "/v1/yards/rollback", br#"{"yardId":"missing"}"#),
    ("POST", "/v1/yards/delete", br#"{"yardId":"missing"}"#),
];

#[tokio::test]
async fn yard_routes_require_the_exact_operator_authority() {
    let unauthorized = test_seams::fixture(&["object:write"]);
    for (method, path, body) in DENIED_ROUTES {
        assert_error(
            send(&unauthorized, method, path, body, false).await,
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        )
        .await;
    }
    assert_reader_authority().await;
}

async fn assert_reader_authority() {
    let reader = test_seams::fixture(&["yard:read"]);
    assert_eq!(
        send(
            &reader,
            "GET",
            "/v1/yards?workspace=fixture&project=project",
            b"",
            false,
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_error(
        send(
            &reader,
            "POST",
            "/v1/yards/delete",
            br#"{"yardId":"missing"}"#,
            false,
        )
        .await,
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
    )
    .await;
}

#[tokio::test]
async fn yard_routes_reject_malformed_or_incomplete_deploys_without_publication() {
    let fixture = test_seams::fixture(&["object:write", "yard:manage"]);
    for body in [
        b"{".as_slice(),
        br#"{"workspace":"fixture","project":"project","name":"docs","clientDeployId":"too-short","spa":false,"cleanUrls":false,"public":true}"#
            .as_slice(),
        br#"{"workspace":"fixture","project":"project","name":"docs","clientDeployId":"client-deploy-0001","spa":false,"cleanUrls":false,"public":false}"#
            .as_slice(),
        br#"{"workspace":"fixture","project":"project","name":"admin","clientDeployId":"client-deploy-0002","spa":false,"cleanUrls":false,"public":true}"#
            .as_slice(),
    ] {
        assert_error(
            send(
                &fixture,
                "POST",
                "/v1/yards/deploys/start",
                body,
                false,
            )
            .await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
    let started = start(&fixture, "empty-deploy-0001").await;
    assert_error(
        send(
            &fixture,
            "POST",
            "/v1/yards/deploys/finalise",
            &serde_json::to_vec(&serde_json::json!({
                "deployId": started["data"]["deployId"]
            }))
            .expect("finalise request"),
            false,
        )
        .await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
}

#[tokio::test]
async fn every_yard_route_maps_extractor_rejections_to_the_public_error_contract() {
    let fixture = test_seams::fixture(&["object:write", "yard:manage"]);
    for path in ["/v1/yards?workspace=fixture", "/v1/yards/deploys"] {
        assert_error(
            send(&fixture, "GET", path, b"", false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
    for path in [
        "/v1/yards/deploys/finalise",
        "/v1/yards/deploys/fail",
        "/v1/yards/rollback",
        "/v1/yards/delete",
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
async fn public_yard_delivery_conceals_unknown_hosts_methods_and_unsafe_paths() {
    let fixture = test_seams::fixture(&["yard:read"]);
    for (method, path, host) in [
        ("GET", "/", "unknown-123456789-fixture"),
        ("POST", "/", "site-123456789-fixture"),
        ("GET", "/../secret", "site-123456789-fixture"),
        ("GET", "/bad//path", "site-123456789-fixture"),
    ] {
        assert_eq!(
            public_request(&fixture, method, path, host, None)
                .await
                .status(),
            StatusCode::NOT_FOUND,
            "{method} {path}"
        );
    }
}
