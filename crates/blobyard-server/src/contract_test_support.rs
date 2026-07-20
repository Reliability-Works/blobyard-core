//! Shared HTTP helpers for the duplicated unit and integration contract suites.

#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use blobyard_server::transfers::test_seams::TransferFixture;
use http_body_util::BodyExt;
use tower::ServiceExt;

/// Sends one authenticated request through the isolated contract fixture.
///
/// # Panics
///
/// Panics when the request cannot be built or served.
pub async fn send(
    fixture: &TransferFixture,
    method: &str,
    path: &str,
    body: &[u8],
    idempotency: bool,
) -> Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, "Bearer secret")
        .header(header::CONTENT_TYPE, "application/json");
    if idempotency {
        request = request.header("idempotency-key", "fixture");
    }
    fixture
        .router()
        .oneshot(request.body(Body::from(body.to_vec())).expect("request"))
        .await
        .expect("response")
}

/// Decodes a fixture response as JSON.
///
/// # Panics
///
/// Panics when the response body cannot be collected or decoded.
pub async fn response_json(response: Response) -> serde_json::Value {
    let body = response
        .into_body()
        .collect()
        .await
        .expect("response body")
        .to_bytes();
    serde_json::from_slice(&body).expect("response JSON")
}

/// Verifies the stable error envelope returned by a contract route.
///
/// # Panics
///
/// Panics when the status or public error code differs from the expectation.
pub async fn assert_error(response: Response, status: StatusCode, code: &str) {
    assert_eq!(response.status(), status);
    let value = response_json(response).await;
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], code);
}
