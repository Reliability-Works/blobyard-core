use super::{
    guest_invite_management_journey as management,
    session_support::{
        body, browser_request, challenge_continuation, path_and_query, private_get, setup,
        submit_login,
    },
};
use crate::contract_test_support::send;
use axum::http::{StatusCode, header};
use blobyard_testkit::FixtureExecutionTracker;

#[tokio::test]
async fn guest_invitation_acceptance_login_and_revocation_are_live() {
    let session = setup().await;
    let mut tracker = FixtureExecutionTracker::new("server", "guest-invitation-http");
    management::approve_application_roles(&session).await;
    management::assert_management_role_outcomes(&session, &mut tracker).await;
    let (invitation_id, invitation_path) =
        management::create_invitation(&session, &mut tracker).await;
    let expired_path = management::create_expired_invitation(&session);
    let (revoked_id, revoked_path) =
        management::create_http_invitation(&session, "revoked@example.com", &[]).await;
    revoke(&session, &revoked_id).await;
    let foreign_path = management::create_foreign_invitation(&session).await;
    let form = open_invitation(&session, &invitation_path, &mut tracker).await;
    super::guest_invite_journey_inputs::assert_rejections(&session, &mut tracker).await;
    super::guest_invite_journey_inputs::assert_token_state_rejections(
        &session,
        &expired_path,
        &revoked_path,
        &invitation_path,
        &foreign_path,
        &mut tracker,
    )
    .await;
    let (guest_key, cookie) = accept_invitation(&session, &form).await;
    assert_replay_is_concealed(&session, &form).await;
    assert_guest_key_login(&session, &guest_key).await;
    assert_management_list_is_redacted(&session, &guest_key, &mut tracker).await;
    assert_guest_key_cannot_authenticate_management(&session, &guest_key, &mut tracker).await;
    revoke(&session, &invitation_id).await;
    assert_eq!(
        private_get(&session, &cookie, true).await.status(),
        StatusCode::FOUND
    );
    let continuation = challenge_continuation(&session).await;
    let denied = submit_login(&session, &continuation, &guest_key, "127.0.0.1:8787").await;
    assert_eq!(denied.status(), StatusCode::OK);
    assert!(
        body(denied)
            .await
            .windows(b"not accepted".len())
            .any(|window| window == b"not accepted")
    );
    tracker.finish().expect("complete guest HTTP fixtures");
}

async fn open_invitation(
    session: &super::session_support::SessionFixture,
    invitation_path: &str,
    tracker: &mut FixtureExecutionTracker,
) -> String {
    let invitation = browser_request(
        &session.fixture,
        "GET",
        invitation_path,
        "127.0.0.1:8787",
        &[],
        "",
        None,
    )
    .await;
    assert_eq!(invitation.status(), StatusCode::OK);
    assert_eq!(invitation.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(invitation.headers()[header::REFERRER_POLICY], "no-referrer");
    assert!(
        invitation.headers()["content-security-policy"]
            .to_str()
            .expect("content security policy")
            .contains("frame-ancestors 'none'")
    );
    tracker.record_case(
        "invitation-pages-prevent-cache-referrer-and-framing-leaks",
        &serde_json::json!({"surface": "account-invitation-page"}),
        &serde_json::json!({
            "cacheControl": "no-store",
            "referrerPolicy": "no-referrer",
            "frameAncestors": "none"
        }),
    );
    let invitation_html = String::from_utf8(body(invitation).await.to_vec()).expect("HTML");
    hidden_invitation_form(&invitation_html)
}

async fn accept_invitation(
    session: &super::session_support::SessionFixture,
    form: &str,
) -> (String, String) {
    let accepted = browser_request(
        &session.fixture,
        "POST",
        "/account/yard-invite/accept",
        "127.0.0.1:8787",
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            (header::ORIGIN.as_str(), "http://127.0.0.1:8787"),
        ],
        form,
        None,
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::OK);
    assert_eq!(accepted.headers()[header::CACHE_CONTROL], "no-store");
    let accepted_html = String::from_utf8(body(accepted).await.to_vec()).expect("HTML");
    let guest_key = between(&accepted_html, "<code>", "</code>");
    assert!(guest_key.starts_with("byg_"));
    assert_eq!(guest_key.len(), 68);
    let exchange_target = between(&accepted_html, "method=\"get\" action=\"", "\">");
    let exchange_code = between(&accepted_html, "name=\"code\" value=\"", "\">");
    assert!(exchange_code.starts_with("byx_"));
    let exchange_url =
        url::Url::parse_with_params(&exchange_target, [("code", exchange_code.as_str())])
            .expect("exchange URL");
    let exchange = browser_request(
        &session.fixture,
        "GET",
        &path_and_query(&exchange_url),
        &format!("{}:8787", session.host),
        &[],
        "",
        None,
    )
    .await;
    assert_eq!(exchange.status(), StatusCode::SEE_OTHER);
    let cookie = exchange.headers()[header::SET_COOKIE]
        .to_str()
        .expect("session cookie")
        .split(';')
        .next()
        .expect("cookie pair")
        .to_owned();
    assert_eq!(
        private_get(session, &cookie, false).await.status(),
        StatusCode::OK
    );
    (guest_key, cookie)
}

async fn assert_replay_is_concealed(session: &super::session_support::SessionFixture, form: &str) {
    let replay = browser_request(
        &session.fixture,
        "POST",
        "/account/yard-invite/accept",
        "127.0.0.1:8787",
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            (header::ORIGIN.as_str(), "http://127.0.0.1:8787"),
        ],
        form,
        None,
    )
    .await;
    let replay_html = String::from_utf8(body(replay).await.to_vec()).expect("HTML");
    assert!(replay_html.contains("Invalid invitation"));
    assert!(!replay_html.contains("byg_"));
}

async fn assert_guest_key_login(session: &super::session_support::SessionFixture, guest_key: &str) {
    let continuation = challenge_continuation(session).await;
    let login = submit_login(session, &continuation, guest_key, "127.0.0.1:8787").await;
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
}

async fn assert_management_list_is_redacted(
    session: &super::session_support::SessionFixture,
    guest_key: &str,
    tracker: &mut FixtureExecutionTracker,
) {
    let response = send(
        &session.fixture,
        "GET",
        &format!("/v1/yards/guest-invites?yardId={}", session.yard_id),
        b"",
        false,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = body(response).await;
    let serialized = std::str::from_utf8(&bytes).expect("JSON");
    assert!(!serialized.contains("bygi_"));
    assert!(!serialized.contains(guest_key));
    assert!(!serialized.contains("secretHash"));
    assert!(!serialized.contains("tokenHash"));
    assert!(!serialized.contains("invitationUrl"));
    tracker.record_case(
        "guest-create-returns-one-time-invitation-url",
        &serde_json::json!({
            "operation": "createYardGuestInvite",
            "responseField": "invitationUrl"
        }),
        &serde_json::json!({
            "presentOnCreate": true,
            "presentOnList": false,
            "reusableAuthorityReturned": false
        }),
    );
}

async fn assert_guest_key_cannot_authenticate_management(
    session: &super::session_support::SessionFixture,
    guest_key: &str,
    tracker: &mut FixtureExecutionTracker,
) {
    let authorization = format!("Bearer {guest_key}");
    let response = browser_request(
        &session.fixture,
        "GET",
        &format!("/v1/yards/guest-invites?yardId={}", session.yard_id),
        "127.0.0.1:8787",
        &[("authorization", &authorization)],
        "",
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    tracker.record_case(
        "guest-key-is-rejected-by-management-authentication",
        &serde_json::json!({
            "principalKind": "guest",
            "surface": "management-authentication"
        }),
        &serde_json::json!({"allowed": false, "code": "UNAUTHORIZED"}),
    );
}

async fn revoke(session: &super::session_support::SessionFixture, invitation_id: &str) {
    let request = serde_json::to_vec(&serde_json::json!({
        "yardId": session.yard_id,
        "invitationId": invitation_id,
    }))
    .expect("revoke request");
    let response = send(
        &session.fixture,
        "POST",
        "/v1/yards/guest-invites/revoke",
        &request,
        false,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

fn hidden_invitation_form(html: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("token", &between(html, "name=\"token\" value=\"", "\">"))
        .append_pair(
            "continuation",
            &between(html, "name=\"continuation\" value=\"", "\">"),
        )
        .finish()
}

fn between(value: &str, start: &str, end: &str) -> String {
    let remainder = value
        .split_once(start)
        .map(|(_prefix, remainder)| remainder)
        .expect("start delimiter");
    remainder
        .split_once(end)
        .map(|(result, _suffix)| result.to_owned())
        .expect("end delimiter")
}
