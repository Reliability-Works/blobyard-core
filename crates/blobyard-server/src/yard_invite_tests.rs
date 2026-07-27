#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::{
    Repository, repository_fault_tests::FaultingRepository, test_support::error_status,
    transfers::test_seams,
};
use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use blobyard_contract::YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS;
use std::sync::Arc;

#[test]
fn invitation_parameters_are_exact_and_unambiguous() {
    assert_eq!(
        super::invite_parameters("token=one&continuation=two"),
        Some(("one".to_owned(), "two".to_owned()))
    );
    assert_eq!(
        super::accept_form("continuation=two&token=one"),
        Some(("one".to_owned(), "two".to_owned()))
    );
    for malformed in [
        "",
        "token=one",
        "continuation=two",
        "token=one&continuation=two&extra=three",
        "token=one&token=two&continuation=three",
        "token=one&continuation=two&continuation=three",
    ] {
        assert_eq!(super::invite_parameters(malformed), None);
        assert_eq!(super::accept_form(malformed), None);
    }
}

#[tokio::test]
async fn invitation_dispatch_propagates_host_clock_resolution_form_and_rate_failures() {
    let fixture = test_seams::fixture(&["yard:read"]);
    assert_invitation_get_failures(&fixture.state);
    assert_acceptance_request_failures(&fixture.state).await;
    assert_acceptance_rate_failures(&fixture.state).await;
}

fn assert_invitation_get_failures(state: &crate::api::AppState) {
    let invalid_host = request(Method::GET, "/account/yard-invite", None, None);
    assert_eq!(
        error_status(super::invitation_at(state, invalid_host, Ok(1))),
        StatusCode::NOT_FOUND
    );
    let valid_get = request(
        Method::GET,
        "/account/yard-invite",
        Some("127.0.0.1:8787"),
        None,
    );
    assert_eq!(
        error_status(super::invitation_at(
            state,
            valid_get,
            Err(crate::error::ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

async fn assert_acceptance_request_failures(state: &crate::api::AppState) {
    let invalid_accept_host = request(Method::POST, "/account/yard-invite/accept", None, None);
    assert_eq!(
        error_status(super::acceptance_at(state, "peer", invalid_accept_host, Ok(1)).await),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        error_status(
            super::parse_request(request(
                Method::POST,
                "/account/yard-invite/accept",
                Some("127.0.0.1:8787"),
                None,
            ))
            .await
        ),
        StatusCode::BAD_REQUEST
    );
    let form = "token=one&continuation=two";
    assert_eq!(
        error_status(
            super::acceptance_at(
                state,
                "peer-clock",
                request(
                    Method::POST,
                    "/account/yard-invite/accept",
                    Some("127.0.0.1:8787"),
                    Some(form),
                ),
                Err(crate::error::ApiError::internal()),
            )
            .await
        ),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

async fn assert_acceptance_rate_failures(state: &crate::api::AppState) {
    let form = "token=one&continuation=two";
    for _ in 0..blobyard_contract::YARD_LOGIN_RATE_LIMIT {
        super::consume_rate_limit(state, "peer-rate", 1).expect("within rate limit");
    }
    assert_eq!(
        error_status(super::consume_rate_limit(state, "peer-rate", 1)),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        error_status(
            super::acceptance_at(
                state,
                "peer-rate",
                request(
                    Method::POST,
                    "/account/yard-invite/accept",
                    Some("127.0.0.1:8787"),
                    Some(form),
                ),
                Ok(1),
            )
            .await
        ),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn invitation_dispatch_propagates_repository_failures_from_both_resolution_paths() {
    let fixture = test_seams::fixture(&["yard:read"]);
    let started = super::test_support::start_yard(&fixture.state);
    let raw_token = format!("bygi_{}", "d".repeat(64));
    let expires_at_ms = 1 + YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS;
    let _invitation = super::test_support::create_invitation(
        &fixture.state,
        &started.yard,
        &raw_token,
        expires_at_ms,
    );
    let continuation = crate::yard_session_contracts::issue_invitation(
        &fixture.state.yard_continuation_key,
        &started.yard.host_label,
        "/",
        1,
        expires_at_ms,
    )
    .expect("continuation");
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("token", &raw_token)
        .append_pair("continuation", continuation.expose_secret())
        .finish();
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);

    let mut get_state = fixture.state.clone();
    get_state.repository = Arc::new(FaultingRepository::new(Arc::clone(&inner), 0));
    let get_path = format!("/account/yard-invite?{encoded}");
    assert_eq!(
        error_status(super::invitation_at(
            &get_state,
            request(Method::GET, &get_path, Some("127.0.0.1:8787"), None),
            Ok(2),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let mut post_state = fixture.state.clone();
    post_state.repository = Arc::new(FaultingRepository::new(inner, 1));
    assert_eq!(
        error_status(
            super::acceptance_at(
                &post_state,
                "peer-resolution",
                request(
                    Method::POST,
                    "/account/yard-invite/accept",
                    Some("127.0.0.1:8787"),
                    Some(&encoded),
                ),
                Ok(2),
            )
            .await
        ),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

fn request(method: Method, uri: &str, host: Option<&str>, form: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(host) = host {
        builder = builder.header(header::HOST, host);
    }
    if form.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    }
    builder
        .body(Body::from(form.unwrap_or_default().to_owned()))
        .expect("request")
}
