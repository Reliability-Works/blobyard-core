#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    callback,
    test_support::{NOW, begin, failure_fixture, guest_fixture, member_fixture},
};
use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use blobyard_testkit::FixtureExecutionTracker;
use http_body_util::BodyExt;
use std::sync::atomic::Ordering;
use tower::ServiceExt;

async fn assert_invalid_link(response: axum::response::Response) {
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("invalid-link body")
        .to_bytes();
    let body = std::str::from_utf8(&body).expect("invalid-link text");
    assert!(
        body.contains("Invalid sign-in link"),
        "a concealed failure must be the exact invalid-link page"
    );
}

async fn successful_callback(
    fixture: &super::test_support::Fixture,
) -> (String, axum::response::Response) {
    let raw_state = begin(fixture).await;
    let response = callback(
        &fixture.state,
        Some(&format!("code=provider-code&state={raw_state}")),
        Ok(NOW + 1),
    )
    .await
    .expect("callback");
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get(header::LOCATION)
        .expect("location")
        .to_str()
        .expect("location text");
    assert!(location.starts_with(&format!(
        "http://{}.localhost:8787/.blobyard/session/exchange?code=byx_",
        fixture.host_label
    )));
    (raw_state, response)
}

async fn assert_exact_route_methods(fixture: &super::test_support::Fixture) {
    for (method, uri) in [
        (Method::PUT, "/account/yard-oidc/start"),
        (Method::POST, "/account/yard-oidc/callback"),
    ] {
        let response = super::routes()
            .with_state(fixture.state.clone())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header(header::HOST, "127.0.0.1:8787")
                    .body(Body::empty())
                    .expect("method request"),
            )
            .await
            .expect("method response");
        assert_invalid_link(response).await;
    }
}

async fn assert_member_callback(
    tracker: &mut FixtureExecutionTracker,
    member: &super::test_support::Fixture,
) -> String {
    assert_exact_route_methods(member).await;
    let malformed = callback(
        &member.state,
        Some("code=provider-code&state=bad&unknown=value"),
        Ok(NOW + 1),
    )
    .await
    .expect("concealed malformed callback");
    assert_invalid_link(malformed).await;
    let (member_state, _response) = successful_callback(member).await;
    tracker.record_case(
        "oidc-start-redirect-and-callback-are-exact",
        &serde_json::json!({
            "startMethod": "POST",
            "callbackMethod": "GET",
            "callbackParameters": ["code", "state"]
        }),
        &serde_json::json!({
            "startStatus": 303,
            "callbackStatus": 303,
            "stateShape": "byos-plus-64-lower-hex"
        }),
    );
    tracker.record_case(
        "oidc-member-callback-issues-yard-exchange-code",
        &serde_json::json!({"candidateKind": "member", "candidateCount": 1}),
        &serde_json::json!({
            "redirectCodeShape": "byx-plus-64-lower-hex",
            "provisioned": false
        }),
    );
    member_state
}

async fn assert_replay_is_concealed(
    tracker: &mut FixtureExecutionTracker,
    member: &super::test_support::Fixture,
    member_state: &str,
) {
    let replay = callback(
        &member.state,
        Some(&format!("code=provider-code&state={member_state}")),
        Ok(NOW + 1),
    )
    .await
    .expect("safe replay failure");
    assert_invalid_link(replay).await;
    assert_eq!(member.provider.exchange_count.load(Ordering::SeqCst), 1);
    tracker.record_case(
        "oidc-malformed-callback-and-replay-are-concealed",
        &serde_json::json!({"callbackShape": "malformed-or-replayed"}),
        &serde_json::json!({
            "responseClass": "invalid-sign-in-link",
            "providerExchangeCountAfterReplay": 1
        }),
    );
}

async fn assert_guest_callback(tracker: &mut FixtureExecutionTracker) {
    let guest = guest_fixture();
    successful_callback(&guest).await;
    tracker.record_case(
        "oidc-accepted-guest-callback-issues-yard-exchange-code",
        &serde_json::json!({"candidateKind": "accepted-guest", "candidateCount": 1}),
        &serde_json::json!({
            "redirectCodeShape": "byx-plus-64-lower-hex",
            "provisioned": false
        }),
    );
}

async fn assert_provider_failure(tracker: &mut FixtureExecutionTracker) {
    let failed = failure_fixture();
    let failed_state = begin(&failed).await;
    for _ in 0..2 {
        let response = callback(
            &failed.state,
            Some(&format!("code=provider-code&state={failed_state}")),
            Ok(NOW + 1),
        )
        .await
        .expect("safe provider failure");
        assert_invalid_link(response).await;
    }
    assert_eq!(failed.provider.exchange_count.load(Ordering::SeqCst), 1);
    tracker.record_case(
        "oidc-provider-or-token-failure-is-concealed-after-claim",
        &serde_json::json!({"providerResult": "invalid-or-unavailable"}),
        &serde_json::json!({
            "responseClass": "invalid-sign-in-link",
            "attemptClaimedBeforeExchange": true
        }),
    );
}

#[tokio::test]
async fn browser_flow_executes_every_generated_oidc_case() {
    let mut tracker = FixtureExecutionTracker::new_oidc("server", "oidc-browser-flow");
    let member = member_fixture();
    let member_state = assert_member_callback(&mut tracker, &member).await;
    assert_replay_is_concealed(&mut tracker, &member, &member_state).await;
    assert_guest_callback(&mut tracker).await;
    assert_provider_failure(&mut tracker).await;
    tracker.finish().expect("browser OIDC fixture coverage");
}
