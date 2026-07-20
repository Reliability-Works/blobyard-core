#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use axum::{
    Router,
    body::Body,
    http::{Request, header},
    response::Response,
};
use tower::ServiceExt;

pub(super) async fn send_as(
    router: Router,
    token: &str,
    method: &str,
    path: &str,
    body: &[u8],
) -> Response {
    router
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_vec()))
                .expect("request"),
        )
        .await
        .expect("response")
}

pub(super) fn item<'a>(value: &'a serde_json::Value, token_id: &str) -> &'a serde_json::Value {
    value["data"]
        .as_array()
        .expect("token list")
        .iter()
        .find(|item| item["id"] == token_id)
        .expect("token summary")
}
