#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::acceptance_at;
use crate::{Repository, repository_fault_tests::FaultingRepository, transfers::test_seams};
use axum::{
    body::{Body, Bytes, to_bytes},
    http::{HeaderValue, Method, Request, StatusCode, header},
    response::IntoResponse,
};
use blobyard_contract::YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS;
use futures_util::stream;
use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Copy)]
enum OriginInput {
    Missing,
    Foreign,
    Malformed,
    NonText,
    Duplicate,
}

#[tokio::test]
async fn acceptance_requires_exact_account_origin_before_body_or_storage() {
    for (case, origin) in [
        ("missing", OriginInput::Missing),
        ("foreign", OriginInput::Foreign),
        ("malformed", OriginInput::Malformed),
        ("non-text", OriginInput::NonText),
        ("duplicate", OriginInput::Duplicate),
    ] {
        assert_origin_rejection(case, origin).await;
    }
}

async fn assert_origin_rejection(case: &str, origin: OriginInput) {
    let fixture = test_seams::fixture(&["yard:read"]);
    let started = super::test_support::start_yard(&fixture.state);
    let raw_token = format!("bygi_{}", "e".repeat(64));
    let expires_at_ms = 1 + YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS;
    let invitation = super::test_support::create_invitation(
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
    let form = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("token", &raw_token)
        .append_pair("continuation", continuation.expose_secret())
        .finish();
    let accepted_audits = accepted_audit_count(&fixture.state);
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let mut rejecting_state = fixture.state.clone();
    rejecting_state.repository = Arc::new(FaultingRepository::new(Arc::clone(&inner), 0));
    let body_consumed = Arc::new(AtomicBool::new(false));
    let request = tracked_request(origin, &form, Arc::clone(&body_consumed));

    let error = acceptance_at(&rejecting_state, case, request, Ok(2))
        .await
        .expect_err("origin rejection");
    assert!(!format!("{error:?}").contains(&raw_token));
    assert_eq!(error.into_response().status(), StatusCode::NOT_FOUND);
    assert!(
        !body_consumed.load(Ordering::SeqCst),
        "{case} Origin consumed the request body"
    );
    assert_eq!(
        inner
            .pending_yard_guest_invite_by_token(&crate::auth::hash(&raw_token), 2)
            .expect("invitation remains pending"),
        invitation
    );
    assert_eq!(accepted_audit_count(&fixture.state), accepted_audits);

    let response = acceptance_at(
        &rejecting_state,
        case,
        tracked_request(origin, &form, Arc::new(AtomicBool::new(false))),
        Ok(2),
    )
    .await
    .expect_err("origin rejection")
    .into_response();
    let response_body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("error body");
    assert!(!String::from_utf8_lossy(&response_body).contains(&raw_token));
}

fn tracked_request(origin: OriginInput, form: &str, consumed: Arc<AtomicBool>) -> Request<Body> {
    let payload = form.to_owned();
    let body = Body::from_stream(stream::once(async move {
        consumed.store(true, Ordering::SeqCst);
        Ok::<Bytes, Infallible>(Bytes::from(payload))
    }));
    let mut request = Request::builder()
        .method(Method::POST)
        .uri("/account/yard-invite/accept")
        .header(header::HOST, "127.0.0.1:8787")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .expect("request");
    match origin {
        OriginInput::Missing => {}
        OriginInput::Foreign => {
            request.headers_mut().insert(
                header::ORIGIN,
                HeaderValue::from_static("https://attacker.example"),
            );
        }
        OriginInput::Malformed => {
            request
                .headers_mut()
                .insert(header::ORIGIN, HeaderValue::from_static("not a URL"));
        }
        OriginInput::NonText => {
            request.headers_mut().insert(
                header::ORIGIN,
                HeaderValue::from_bytes(b"\xff").expect("non-text header"),
            );
        }
        OriginInput::Duplicate => {
            for _ in 0..2 {
                request.headers_mut().append(
                    header::ORIGIN,
                    HeaderValue::from_static("http://127.0.0.1:8787"),
                );
            }
        }
    }
    request
}

fn accepted_audit_count(state: &crate::api::AppState) -> usize {
    state
        .repository
        .list_audit(&state.default_workspace.id, None, 100)
        .expect("audit page")
        .items
        .into_iter()
        .filter(|event| event.action == "yard.guest_invite.accepted")
        .count()
}
