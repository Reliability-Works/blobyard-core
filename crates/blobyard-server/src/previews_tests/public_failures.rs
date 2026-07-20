use super::super::public_fallback_at;
use crate::{error::ApiError, transfers::test_seams};
use axum::{
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::IntoResponse,
};

#[tokio::test]
async fn public_preview_resolver_propagates_clock_failure_before_repository_access() {
    let fixture = test_seams::fixture(&["share:manage"]);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::HOST,
        format!("{}.localhost:8787", "a".repeat(52))
            .parse()
            .expect("preview host"),
    );
    for method in [Method::GET, Method::HEAD] {
        assert_eq!(
            public_fallback_at(
                &fixture.state,
                &"/".parse().expect("preview path"),
                &method,
                &headers,
                Err(ApiError::internal()),
            )
            .await
            .expect_err("clock failure")
            .into_response()
            .status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[tokio::test]
async fn public_preview_resolver_conceals_missing_and_non_text_hosts() {
    let fixture = test_seams::fixture(&["share:manage"]);
    for headers in [HeaderMap::new(), non_text_host_headers()] {
        assert_eq!(
            public_fallback_at(
                &fixture.state,
                &"/".parse().expect("preview path"),
                &Method::GET,
                &headers,
                Ok(1_000),
            )
            .await
            .expect_err("invalid host")
            .into_response()
            .status(),
            StatusCode::NOT_FOUND
        );
    }
}

fn non_text_host_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::HOST,
        HeaderValue::from_bytes(&[0xff]).expect("opaque host value"),
    );
    headers
}
