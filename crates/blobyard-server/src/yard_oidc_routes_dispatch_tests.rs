#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::super::{routes, test_support::member_fixture};
use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use tower::ServiceExt;

#[tokio::test]
async fn wrong_dispatch_context_is_not_found() {
    let fixture = member_fixture();
    for request in [
        Request::builder()
            .method(Method::PUT)
            .uri("/account/yard-oidc/start")
            .header(header::HOST, "foreign.example.test")
            .body(Body::empty())
            .expect("foreign start host"),
        Request::builder()
            .method(Method::POST)
            .uri("/account/yard-oidc/start")
            .header(header::HOST, "127.0.0.1:8787")
            .header(header::ORIGIN, "https://foreign.example.test")
            .body(Body::empty())
            .expect("foreign start origin"),
        Request::builder()
            .method(Method::GET)
            .uri("/account/yard-oidc/callback")
            .header(header::HOST, "foreign.example.test")
            .body(Body::empty())
            .expect("foreign callback host"),
    ] {
        assert_eq!(
            routes()
                .with_state(fixture.state.clone())
                .oneshot(request)
                .await
                .expect("rejected dispatch")
                .status(),
            StatusCode::NOT_FOUND
        );
    }
}
