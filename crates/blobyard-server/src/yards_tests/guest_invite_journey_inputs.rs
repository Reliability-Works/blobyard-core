#![allow(
    clippy::cognitive_complexity,
    clippy::too_many_lines,
    reason = "the test helper replays one ordered hostile-input journey"
)]

use super::{
    guest_invite_management_journey::guest_invitation_items,
    session_support::{body, browser_request, challenge_continuation, submit_login},
};
use axum::http::StatusCode;
use blobyard_testkit::FixtureExecutionTracker;

pub(super) async fn assert_rejections(
    session: &super::session_support::SessionFixture,
    tracker: &mut FixtureExecutionTracker,
) {
    for (method, path) in [
        ("POST", "/account/yard-invite"),
        ("GET", "/account/yard-invite/accept"),
    ] {
        let response = browser_request(
            &session.fixture,
            method,
            path,
            "127.0.0.1:8787",
            &[],
            "",
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    let missing = browser_request(
        &session.fixture,
        "GET",
        "/account/yard-invite",
        "127.0.0.1:8787",
        &[],
        "",
        None,
    )
    .await;
    assert_invalid_invitation(missing).await;

    let ambiguous = browser_request(
        &session.fixture,
        "POST",
        "/account/yard-invite/accept",
        "127.0.0.1:8787",
        &[("content-type", "application/x-www-form-urlencoded")],
        "token=one&token=two&continuation=three",
        None,
    )
    .await;
    assert_invalid_invitation(ambiguous).await;
    tracker.record_case(
        "malformed-and-ambiguous-invitation-input-is-concealed",
        &serde_json::json!({
            "surface": "account-acceptance",
            "inputClass": ["malformed", "ambiguous"]
        }),
        &serde_json::json!({
            "responseClass": "concealed-not-found",
            "authorityCreated": false
        }),
    );

    let oversized_body = "a".repeat(32 * 1_024 + 1);
    let oversized = browser_request(
        &session.fixture,
        "POST",
        "/account/yard-invite/accept",
        "127.0.0.1:8787",
        &[("content-type", "application/x-www-form-urlencoded")],
        &oversized_body,
        None,
    )
    .await;
    assert_eq!(oversized.status(), StatusCode::BAD_REQUEST);
    tracker.record_case(
        "oversized-invitation-submission-is-rejected-before-resolution",
        &serde_json::json!({"surface": "account-acceptance", "bodyBytes": 32769}),
        &serde_json::json!({"responseCode": "BAD_REQUEST", "repositoryCalls": 0}),
    );

    let invalid_limit = browser_request(
        &session.fixture,
        "GET",
        &format!("/v1/yards/guest-invites?yardId={}&limit=0", session.yard_id),
        "127.0.0.1:8787",
        &[("authorization", "Bearer secret")],
        "",
        None,
    )
    .await;
    assert_eq!(invalid_limit.status(), StatusCode::BAD_REQUEST);

    let continuation = challenge_continuation(session).await;
    let malformed_guest_key =
        submit_login(session, &continuation, "byg_short", "127.0.0.1:8787").await;
    assert_eq!(malformed_guest_key.status(), StatusCode::OK);
    let malformed_body = body(malformed_guest_key).await;
    assert!(
        malformed_body
            .windows(b"not accepted".len())
            .any(|window| window == b"not accepted")
    );
}

pub(super) async fn assert_token_state_rejections(
    session: &super::session_support::SessionFixture,
    expired_path: &str,
    revoked_path: &str,
    valid_path: &str,
    foreign_path: &str,
    tracker: &mut FixtureExecutionTracker,
) {
    let accepted_before = accepted_invitation_count(session).await;
    let foreign_url = url::Url::parse(&format!("http://localhost{foreign_path}"))
        .expect("foreign invitation URL");
    let foreign_token = query_value(&foreign_url, "token");
    for (case_id, invitation_state, form) in [
        (
            "expired-guest-invitation-token-is-concealed",
            "expired",
            invitation_form(expired_path, None),
        ),
        (
            "revoked-guest-invitation-token-is-concealed",
            "revoked",
            invitation_form(revoked_path, None),
        ),
        (
            "foreign-guest-invitation-token-is-concealed",
            "foreign",
            invitation_form(valid_path, Some(&foreign_token)),
        ),
        (
            "unknown-guest-invitation-token-is-concealed",
            "unknown",
            invitation_form(valid_path, Some(&format!("bygi_{}", "z".repeat(64)))),
        ),
    ] {
        let rejected = browser_request(
            &session.fixture,
            "POST",
            "/account/yard-invite/accept",
            "127.0.0.1:8787",
            &[("content-type", "application/x-www-form-urlencoded")],
            &form,
            None,
        )
        .await;
        assert_invalid_invitation(rejected).await;
        assert_eq!(accepted_invitation_count(session).await, accepted_before);
        tracker.record_case(
            case_id,
            &serde_json::json!({
                "surface": "account-acceptance",
                "invitationState": invitation_state
            }),
            &serde_json::json!({
                "responseClass": "concealed-not-found",
                "authorityCreated": false
            }),
        );
    }
}

fn invitation_form(path: &str, token_override: Option<&str>) -> String {
    let url = url::Url::parse(&format!("http://localhost{path}")).expect("invitation URL");
    let token = token_override.map_or_else(
        || query_value(&url, "token"),
        std::borrow::ToOwned::to_owned,
    );
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("token", &token)
        .append_pair("continuation", &query_value(&url, "continuation"))
        .finish()
}

fn query_value(url: &url::Url, name: &str) -> String {
    url.query_pairs()
        .find_map(|(key, value)| (key == name).then(|| value.into_owned()))
        .expect("invitation query value")
}

async fn accepted_invitation_count(session: &super::session_support::SessionFixture) -> usize {
    guest_invitation_items(session)
        .await
        .iter()
        .filter(|invitation| invitation["status"] == "accepted")
        .count()
}

async fn assert_invalid_invitation(response: axum::response::Response) {
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        body(response)
            .await
            .windows(b"Invalid invitation".len())
            .any(|window| window == b"Invalid invitation")
    );
}
