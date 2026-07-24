use super::{
    mutate,
    session_support::{
        SessionFixture, assert_identity_login_location, body, browser_request,
        challenge_continuation, private_get, setup, sign_in, submit_login, uniform_failure_headers,
    },
};
use crate::contract_test_support::send;
use axum::http::{StatusCode, header};

#[tokio::test]
async fn redirects_are_html_get_only_and_logout_is_idempotent() {
    let session = setup().await;
    assert_redirect_matrix(&session).await;
    assert_logout(&session).await;
    let reserved = browser_request(
        &session.fixture,
        "GET",
        "/.blobyard/not-a-runtime-route",
        &format!("{}:8787", session.host),
        &[("accept", "text/html")],
        "",
        None,
    )
    .await;
    assert_eq!(reserved.status(), StatusCode::NOT_FOUND);
}

async fn assert_redirect_matrix(session: &SessionFixture) {
    for (method, accept, expected) in [
        ("GET", Some("text/html"), StatusCode::FOUND),
        ("GET", Some("application/json"), StatusCode::NOT_FOUND),
        ("HEAD", Some("text/html"), StatusCode::NOT_FOUND),
        ("POST", Some("text/html"), StatusCode::NOT_FOUND),
    ] {
        let headers = accept.map_or_else(Vec::new, |value| vec![("accept", value)]);
        let response = browser_request(
            &session.fixture,
            method,
            "/",
            "unknown-123456789-fixture.localhost:8787",
            &headers,
            "",
            None,
        )
        .await;
        assert_eq!(response.status(), expected, "{method} {accept:?}");
        if expected == StatusCode::FOUND {
            assert_identity_login_location(response.headers());
        }
    }
}

async fn assert_logout(session: &SessionFixture) {
    let signed_in = sign_in(session, "/").await;
    for cookie in [Some(signed_in.cookie.as_str()), None] {
        let response = browser_request(
            &session.fixture,
            "POST",
            "/.blobyard/session/logout",
            &format!("{}:8787", session.host),
            &[(
                header::ORIGIN.as_str(),
                &format!("http://{}:8787", session.host),
            )],
            "",
            cookie,
        )
        .await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()[header::LOCATION], "/");
        assert_eq!(
            response.headers()[header::SET_COOKIE],
            "__Host-blobyard-yard-session=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0"
        );
    }
    assert_eq!(
        private_get(session, &signed_in.cookie, true).await.status(),
        StatusCode::FOUND
    );
}

#[tokio::test]
async fn owner_denial_and_login_key_failures_do_not_leak_account_state() {
    let session = setup().await;
    let continuation = challenge_continuation(&session).await;
    let login_page = browser_request(
        &session.fixture,
        "GET",
        &format!("/account/yard-login?continuation={continuation}"),
        "127.0.0.1:8787",
        &[],
        "",
        None,
    )
    .await;
    assert!(contains(&body(login_page).await, b">Sign-in key</label>"));
    assert_owner_denied(&session, &continuation).await;
    let wrong_host = submit_login(
        &session,
        &continuation,
        &session.login_key,
        &format!("{}:8787", session.host),
    )
    .await;
    assert_eq!(wrong_host.status(), StatusCode::NOT_FOUND);
    assert_uniform_key_failures(&session, &continuation).await;
}

async fn assert_owner_denied(session: &SessionFixture, continuation: &str) {
    mutate(
        &session.fixture,
        "/v1/yards/access/visibility",
        serde_json::json!({ "yardId": session.yard_id, "visibility": "owner" }),
    )
    .await;
    let denied = submit_login(session, continuation, &session.login_key, "127.0.0.1:8787").await;
    assert_eq!(denied.status(), StatusCode::OK);
    let denied = body(denied).await;
    assert!(contains(&denied, b"Access denied"));
    assert!(contains(
        &denied,
        b"You do not have access to this Yard, or it does not exist."
    ));
    mutate(
        &session.fixture,
        "/v1/yards/access/visibility",
        serde_json::json!({ "yardId": session.yard_id, "visibility": "selected" }),
    )
    .await;
}

async fn assert_uniform_key_failures(session: &SessionFixture, continuation: &str) {
    let unknown = submit_login(
        session,
        continuation,
        "byuk_ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "127.0.0.1:8787",
    )
    .await;
    let unknown_headers = uniform_failure_headers(&unknown);
    let unknown_body = body(unknown).await;
    assert!(contains(
        &unknown_body,
        b"That sign-in key was not accepted"
    ));
    let deactivate_body = serde_json::to_vec(&serde_json::json!({
        "userId": session.user_id,
    }))
    .expect("deactivation body");
    assert_eq!(
        send(
            &session.fixture,
            "POST",
            "/v1/users/deactivate",
            &deactivate_body,
            false,
        )
        .await
        .status(),
        StatusCode::OK
    );
    let deactivated =
        submit_login(session, continuation, &session.login_key, "127.0.0.1:8787").await;
    assert_eq!(uniform_failure_headers(&deactivated), unknown_headers);
    assert_eq!(body(deactivated).await, unknown_body);
}

#[tokio::test]
async fn login_rate_limit_runs_before_continuation_verification() {
    let session = setup().await;
    for attempt in 1..=11 {
        let response = submit_login(
            &session,
            "not-a-signed-continuation",
            "not-a-login-key",
            "127.0.0.1:8787",
        )
        .await;
        if attempt <= 10 {
            assert_eq!(response.status(), StatusCode::OK);
            let page = body(response).await;
            assert!(contains(
                &page,
                b"This sign-in link is not valid or has expired"
            ));
        } else {
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
            assert!(response.headers().contains_key(header::RETRY_AFTER));
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|part| part == needle)
}
