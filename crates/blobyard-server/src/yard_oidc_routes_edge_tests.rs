#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    callback, repository_lookup, repository_write, start, start_redirect,
    test_support::{NOW, begin, member_fixture, start_request},
};
use crate::{
    error::ApiError,
    repository_fault_tests::FaultingRepository,
    yard_oidc_provider::{
        YardOidcAuthorization, YardOidcExchangeFuture, YardOidcProvider, YardOidcProviderError,
    },
};
use axum::{
    body::{Body, Bytes},
    http::{Method, Request, StatusCode, header},
};
use blobyard_contract::RepositoryError;
use blobyard_core::SecretString;
use futures_util::stream;
use std::{io, sync::Arc};
use tower::ServiceExt;

struct AuthorizationFailure;

impl YardOidcProvider for AuthorizationFailure {
    fn authorization_url(
        &self,
        _authorization: &YardOidcAuthorization,
    ) -> Result<String, YardOidcProviderError> {
        Err(YardOidcProviderError::Unavailable)
    }

    fn exchange(
        &self,
        _code: SecretString,
        _nonce: SecretString,
        _pkce_verifier: SecretString,
        _now_ms: u64,
    ) -> YardOidcExchangeFuture<'_> {
        Box::pin(async { Err(YardOidcProviderError::InvalidResponse) })
    }
}

#[test]
fn repository_lookup_classification_is_exact() {
    assert!(matches!(repository_lookup(Ok(7)), Ok(Some(7))));
    assert!(matches!(
        repository_lookup::<()>(Err(RepositoryError::NotFound)),
        Ok(None)
    ));
    assert!(matches!(
        repository_lookup::<()>(Err(RepositoryError::InvalidInput)),
        Ok(None)
    ));
    assert!(repository_lookup::<()>(Err(RepositoryError::Unavailable)).is_err());
}

#[test]
fn repository_write_classification_and_start_conflicts_are_exact() {
    assert!(matches!(repository_write(Ok(())), Ok(true)));
    assert!(matches!(
        repository_write(Err(RepositoryError::Conflict)),
        Ok(false)
    ));
    assert!(matches!(
        repository_write(Err(RepositoryError::NotFound)),
        Ok(false)
    ));
    assert_eq!(
        start_redirect(
            "https://identity.example.test/authorize",
            Err(RepositoryError::Conflict),
        )
        .expect("concealed conflict")
        .status(),
        StatusCode::OK
    );
    assert!(
        start_redirect(
            "https://identity.example.test/authorize",
            Err(RepositoryError::Unavailable),
        )
        .is_err()
    );
    assert!(repository_write(Err(RepositoryError::Unavailable)).is_err());
}

#[tokio::test]
async fn exact_dispatch_methods_reach_the_handlers() {
    let fixture = member_fixture();
    let post = Request::builder()
        .method(Method::POST)
        .uri("/account/yard-oidc/start")
        .header(header::HOST, "127.0.0.1:8787")
        .header(header::ORIGIN, &fixture.state.public_origin)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("unknown=value"))
        .expect("start dispatch request");
    assert_eq!(
        super::routes()
            .with_state(fixture.state.clone())
            .oneshot(post)
            .await
            .expect("start dispatch")
            .status(),
        StatusCode::OK
    );
    let get = Request::builder()
        .method(Method::GET)
        .uri("/account/yard-oidc/callback?unknown=value")
        .header(header::HOST, "127.0.0.1:8787")
        .body(Body::empty())
        .expect("callback dispatch request");
    assert_eq!(
        super::routes()
            .with_state(fixture.state.clone())
            .oneshot(get)
            .await
            .expect("callback dispatch")
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn invalid_start_inputs_are_concealed_before_provider_effects() {
    let fixture = member_fixture();
    let mut disabled = fixture.state.clone();
    disabled.yard_oidc_provider = None;
    assert_eq!(
        start(
            &disabled,
            "fingerprint",
            start_request(&fixture.continuation),
            Ok(NOW),
        )
        .await
        .expect("disabled provider")
        .status(),
        StatusCode::OK
    );
    for request in [
        Request::new(Body::empty()),
        form_request("unknown=value"),
        form_request("continuation="),
        form_request("continuation=invalid"),
    ] {
        assert_eq!(
            start(&fixture.state, "fingerprint", request, Ok(NOW))
                .await
                .expect("invalid start")
                .status(),
            StatusCode::OK
        );
    }
    let failed_body = Body::from_stream(stream::once(async {
        Err::<Bytes, io::Error>(io::Error::other("fixture body failure"))
    }));
    let failed_request = Request::builder()
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(failed_body)
        .expect("failed body request");
    assert!(
        start(&fixture.state, "fingerprint", failed_request, Ok(NOW))
            .await
            .is_err()
    );
    assert!(
        start(
            &fixture.state,
            "fingerprint",
            start_request(&fixture.continuation),
            Err(ApiError::internal()),
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn provider_and_rate_limit_failures_remain_redaction_safe() {
    let fixture = member_fixture();
    let mut authorization_failure = fixture.state.clone();
    authorization_failure.yard_oidc_provider = Some(Arc::new(AuthorizationFailure));
    assert!(
        start(
            &authorization_failure,
            "fingerprint",
            start_request(&fixture.continuation),
            Ok(NOW),
        )
        .await
        .is_err()
    );

    let mut rate_failure = fixture.state.clone();
    rate_failure.repository = Arc::new(FaultingRepository::new(
        Arc::clone(&fixture.state.repository),
        0,
    ));
    assert!(
        start(
            &rate_failure,
            "fingerprint",
            start_request(&fixture.continuation),
            Ok(NOW),
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn callback_input_and_provider_failures_remain_redaction_safe() {
    let fixture = member_fixture();
    let mut disabled = fixture.state.clone();
    disabled.yard_oidc_provider = None;
    assert_eq!(
        callback(
            &disabled,
            Some(&format!("code=provider-code&state=byos_{}", "a".repeat(64))),
            Ok(NOW + 1),
        )
        .await
        .expect("disabled callback")
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        callback(
            &fixture.state,
            Some(&format!("code=&state=byos_{}", "a".repeat(64))),
            Ok(NOW + 1),
        )
        .await
        .expect("empty code")
        .status(),
        StatusCode::OK
    );

    let raw_state = begin(&fixture).await;
    let query = format!("code=provider-code&state={raw_state}");
    assert!(
        callback(&fixture.state, Some(&query), Err(ApiError::internal()))
            .await
            .is_err()
    );
    assert_eq!(
        callback(
            &fixture.state,
            Some(&format!("code=wrong-code&state={raw_state}")),
            Ok(NOW + 1),
        )
        .await
        .expect("invalid provider response")
        .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn callback_repository_failures_remain_redaction_safe() {
    for failure_index in 0..=3 {
        let failed = member_fixture();
        let state_value = begin(&failed).await;
        let mut state = failed.state.clone();
        state.repository = Arc::new(FaultingRepository::new(
            Arc::clone(&failed.state.repository),
            failure_index,
        ));
        assert!(
            callback(
                &state,
                Some(&format!("code=provider-code&state={state_value}")),
                Ok(NOW + 1),
            )
            .await
            .is_err(),
            "repository failure {failure_index}"
        );
    }
}

fn form_request(body: &'static str) -> Request<Body> {
    Request::builder()
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .expect("form request")
}

#[path = "yard_oidc_routes_dispatch_tests.rs"]
mod dispatch_tests;
