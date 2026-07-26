//! Malformed browser-session request coverage.

use super::{
    faulted_state,
    session_support::{
        browser_request, challenge_continuation, exchange_location, path_and_query, setup, sign_in,
    },
};
use crate::transfers::test_seams;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use tower::ServiceExt;

const IDENTITY_HOST: &str = "127.0.0.1:8787";

#[test]
fn browser_fixture_preserves_paths_with_and_without_queries() {
    assert_eq!(
        path_and_query(&url::Url::parse("http://example.test/path").expect("URL")),
        "/path"
    );
    assert_eq!(
        path_and_query(&url::Url::parse("http://example.test/path?key=value").expect("URL")),
        "/path?key=value"
    );
}

#[tokio::test]
async fn login_route_rejects_unsupported_and_malformed_requests() {
    let fixture = test_seams::fixture(&["yard:read"]);
    for (method, path) in [
        ("DELETE", "/account/yard-login"),
        ("GET", "/account/yard-login"),
        ("GET", "/account/yard-login?wrong=value"),
        (
            "GET",
            "/account/yard-login?continuation=one&continuation=two",
        ),
        ("GET", "/account/yard-login?continuation=not-signed"),
    ] {
        let response = browser_request(&fixture, method, path, IDENTITY_HOST, &[], "", None).await;
        assert_eq!(
            response.status(),
            if method == "DELETE" {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::OK
            }
        );
    }

    for (content_type, body) in [
        ("application/json", "{}"),
        ("application/x-www-form-urlencoded", "continuation=one"),
        (
            "application/x-www-form-urlencoded",
            "continuation=not-signed&login_key=not-secret",
        ),
    ] {
        let fixture = test_seams::fixture(&["yard:read"]);
        let response = browser_request(
            &fixture,
            "POST",
            "/account/yard-login",
            IDENTITY_HOST,
            &[("content-type", content_type)],
            body,
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
}

#[tokio::test]
async fn login_route_bounds_and_decodes_form_bodies() {
    for body in [vec![0xff], vec![b'a'; 32 * 1_024 + 1]] {
        let fixture = test_seams::fixture(&["yard:read"]);
        let response = fixture
            .router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/account/yard-login")
                    .header(header::HOST, IDENTITY_HOST)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(body))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn yard_session_routes_reject_unsupported_methods_and_malformed_hosts() {
    let fixture = test_seams::fixture(&["yard:read"]);
    for (method, path, host) in [
        (
            "POST",
            "/.blobyard/session/exchange",
            "documentation-x.blobyard.test:8787",
        ),
        (
            "GET",
            "/.blobyard/session/logout",
            "documentation-x.blobyard.test:8787",
        ),
        ("GET", "/.blobyard/session/exchange", IDENTITY_HOST),
        ("POST", "/.blobyard/session/logout", IDENTITY_HOST),
    ] {
        let response = browser_request(&fixture, method, path, host, &[], "", None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn login_surfaces_each_unavailable_repository_stage() {
    let session = setup().await;
    let continuation = challenge_continuation(&session).await;
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("continuation", &continuation)
        .append_pair("login_key", &session.login_key)
        .finish();
    for failure_index in 1..=3 {
        let response =
            crate::api::router_with_state(faulted_state(&session.fixture, failure_index))
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/account/yard-login")
                        .header(header::HOST, IDENTITY_HOST)
                        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                        .body(Body::from(body.clone()))
                        .expect("request"),
                )
                .await
                .expect("response");
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}

#[tokio::test]
async fn valid_yard_host_redirects_missing_and_malformed_exchange_codes() {
    let session = setup().await;
    let host = format!("{}:8787", session.host);
    for path in [
        "/.blobyard/session/exchange",
        "/.blobyard/session/exchange?wrong=value",
        "/.blobyard/session/exchange?code=malformed",
        "/.blobyard/session/exchange?code=one&code=two",
    ] {
        let response = browser_request(&session.fixture, "GET", path, &host, &[], "", None).await;
        assert_eq!(response.status(), StatusCode::FOUND);
    }
    let logout = browser_request(
        &session.fixture,
        "POST",
        "/.blobyard/session/logout",
        &host,
        &[],
        "",
        None,
    )
    .await;
    assert_eq!(logout.status(), StatusCode::SEE_OTHER);

    let signed_in = sign_in(&session, "/").await;
    let unavailable = crate::api::router_with_state(faulted_state(&session.fixture, 0))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/.blobyard/session/logout")
                .header(header::HOST, &host)
                .header(header::ORIGIN, format!("http://{host}"))
                .header(header::COOKIE, signed_in.cookie)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(unavailable.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn exchange_surfaces_unavailable_repository_failures() {
    let session = setup().await;
    let exchange_url = exchange_location(&session, "/").await;
    let exchange_url = url::Url::parse(&exchange_url).expect("exchange URL");
    let response = crate::api::router_with_state(faulted_state(&session.fixture, 0))
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(path_and_query(&exchange_url))
                .header(header::HOST, format!("{}:8787", session.host))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
