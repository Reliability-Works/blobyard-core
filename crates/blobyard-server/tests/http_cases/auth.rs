use crate::support::{authorized_server, send, send_json_bytes};
use axum::{
    body::Body,
    http::{HeaderValue, Request, StatusCode, header},
};
use blobyard_server::initialize;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn bootstrap_is_one_time_and_authentication_fails_closed() {
    let server = authorized_server().await;
    let health = send(&server.router, "GET", "/v1/health", None, None).await;
    assert_eq!(health.0, StatusCode::OK);
    assert_eq!(health.1["data"]["status"], "ok");

    let unauthenticated = send(&server.router, "GET", "/v1/cli/whoami", None, None).await;
    assert_eq!(unauthenticated.0, StatusCode::UNAUTHORIZED);
    assert_eq!(unauthenticated.1["error"]["code"], "AUTH_REQUIRED");

    let malformed = server
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/cli/whoami")
                .header(
                    header::AUTHORIZATION,
                    HeaderValue::from_bytes(&[0xff]).expect("opaque header value"),
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(malformed.status(), StatusCode::UNAUTHORIZED);

    let replay = send(
        &server.router,
        "POST",
        "/v1/bootstrap/exchange",
        Some(json!({
            "name": "Replay",
            "platform": "test",
            "token": server.bootstrap_token,
            "version": "0.0.0-test"
        })),
        None,
    )
    .await;
    assert_eq!(replay.0, StatusCode::UNAUTHORIZED);
    assert_eq!(replay.1["error"]["code"], "INVALID_TOKEN");

    let identity = send(
        &server.router,
        "GET",
        "/v1/cli/whoami",
        None,
        Some(&server.access_token),
    )
    .await;
    assert_eq!(identity.0, StatusCode::OK);
    assert_eq!(identity.1["data"]["defaultWorkspace"]["slug"], "default");

    let mut reopened = initialize(server.temporary.path()).expect("reopened server");
    assert!(reopened.take_bootstrap_token().is_none());
}

#[tokio::test]
async fn scoped_operator_can_create_and_list_namespaces() {
    let server = authorized_server().await;

    let create = send(
        &server.router,
        "POST",
        "/v1/workspaces",
        Some(json!({ "name": "Release Builds" })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(create.0, StatusCode::OK);
    assert_eq!(create.1["data"]["slug"], "release-builds");

    let project = send(
        &server.router,
        "POST",
        "/v1/projects",
        Some(json!({ "workspace": "release-builds", "name": "Documentation" })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(project.0, StatusCode::OK);
    assert_eq!(project.1["data"]["slug"], "documentation");

    let projects = send(
        &server.router,
        "GET",
        "/v1/projects?workspace=release-builds",
        None,
        Some(&server.access_token),
    )
    .await;
    assert_eq!(projects.0, StatusCode::OK);
    assert_eq!(
        projects.1["data"]["items"].as_array().map(Vec::len),
        Some(1)
    );

    let list = send(
        &server.router,
        "GET",
        "/v1/workspaces",
        None,
        Some(&server.access_token),
    )
    .await;
    assert_eq!(list.0, StatusCode::OK);
    assert_eq!(list.1["data"]["items"].as_array().map(Vec::len), Some(2));
}

#[tokio::test]
async fn malformed_json_uses_the_stable_error_envelope() {
    let server = authorized_server().await;
    let malformed = send_json_bytes(
        &server.router,
        "POST",
        "/v1/workspaces",
        b"{not-json".to_vec(),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(malformed.0, StatusCode::BAD_REQUEST);
    assert_eq!(malformed.1["error"]["code"], "INVALID_REQUEST");
}
