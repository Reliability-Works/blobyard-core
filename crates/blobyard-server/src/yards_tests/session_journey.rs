use super::{host, journey_tests::publish, mutate};
use crate::{
    contract_test_support::{response_json, send},
    transfers::test_seams::{self, TransferFixture},
};
use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
    response::Response,
};
use http_body_util::BodyExt;
use tower::ServiceExt;

struct SessionFixture {
    deployment_host: String,
    fixture: TransferFixture,
    host: String,
    yard_id: String,
    login_key: String,
    user_id: String,
}

struct SignedIn {
    cookie: String,
    exchange_path: String,
}

#[tokio::test]
async fn private_yard_login_exchange_management_and_revocation_are_live() {
    let session = setup().await;
    let signed_in = sign_in(&session, "/docs/?q=one").await;
    let replay = browser_request(
        &session.fixture,
        "GET",
        &signed_in.exchange_path,
        &format!("{}:8787", session.host),
        &[],
        "",
        None,
    )
    .await;
    assert_eq!(replay.status(), StatusCode::FOUND);
    assert_identity_login_location(replay.headers());

    let served = private_get(&session, &signed_in.cookie, false).await;
    assert_eq!(served.status(), StatusCode::OK);
    assert_eq!(body(served).await.as_ref(), b"docs index");
    let wrong_host_cookie = browser_request(
        &session.fixture,
        "GET",
        "/docs/",
        &format!("{}:8787", session.deployment_host),
        &[("accept", "text/html")],
        "",
        Some(&signed_in.cookie),
    )
    .await;
    assert_eq!(wrong_host_cookie.status(), StatusCode::FOUND);

    let mismatched_logout = browser_request(
        &session.fixture,
        "POST",
        "/.blobyard/session/logout",
        &format!("{}:8787", session.host),
        &[(header::ORIGIN.as_str(), "https://attacker.example")],
        "",
        Some(&signed_in.cookie),
    )
    .await;
    assert_eq!(mismatched_logout.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        private_get(&session, &signed_in.cookie, false)
            .await
            .status(),
        StatusCode::OK
    );

    assert_list_and_management_revoke(&session, &signed_in.cookie).await;
}

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
    assert!(
        body(denied)
            .await
            .windows(13)
            .any(|part| part == b"Access denied")
    );
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
        } else {
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
            assert!(response.headers().contains_key(header::RETRY_AFTER));
        }
    }
}

async fn setup() -> SessionFixture {
    let fixture =
        test_seams::fixture(&["object:write", "users:manage", "yard:manage", "yard:read"]);
    let published = publish(&fixture, "deploy-session-001", b"private index").await;
    let deployment_host = host(&published, "deploymentUrl");
    let host = host(&published, "url");
    let yard_id = published["data"]["yardId"]
        .as_str()
        .expect("yard ID")
        .to_owned();
    let created = response_json(
        send(
            &fixture,
            "POST",
            "/v1/users",
            br#"{"displayName":"Yard Reader","workspace":"fixture"}"#,
            false,
        )
        .await,
    )
    .await;
    let login_key = created["data"]["loginKey"]
        .as_str()
        .expect("login key")
        .to_owned();
    let user_id = created["data"]["user"]["id"]
        .as_str()
        .expect("user ID")
        .to_owned();
    mutate(
        &fixture,
        "/v1/yards/access/visibility",
        serde_json::json!({ "yardId": yard_id, "visibility": "selected" }),
    )
    .await;
    mutate(
        &fixture,
        "/v1/yards/access/grant",
        serde_json::json!({
            "yardId": yard_id,
            "principalKind": "user",
            "principalId": user_id,
            "appRoles": []
        }),
    )
    .await;
    SessionFixture {
        deployment_host,
        fixture,
        host,
        yard_id,
        login_key,
        user_id,
    }
}

async fn sign_in(session: &SessionFixture, return_path: &str) -> SignedIn {
    let challenge = browser_request(
        &session.fixture,
        "GET",
        return_path,
        &format!("{}:8787", session.host),
        &[("accept", "text/html")],
        "",
        None,
    )
    .await;
    assert_eq!(challenge.status(), StatusCode::FOUND);
    let continuation = continuation_from(&challenge);
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("continuation", &continuation)
        .append_pair("login_key", &session.login_key)
        .finish();
    let login = browser_request(
        &session.fixture,
        "POST",
        "/account/yard-login",
        "127.0.0.1:8787",
        &[("content-type", "application/x-www-form-urlencoded")],
        &body,
        None,
    )
    .await;
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    assert!(login.headers().get(header::SET_COOKIE).is_none());
    let exchange_url = login.headers()[header::LOCATION]
        .to_str()
        .expect("exchange URL")
        .to_owned();
    exchange_code(session, &exchange_url, return_path).await
}

async fn exchange_code(
    session: &SessionFixture,
    exchange_url: &str,
    return_path: &str,
) -> SignedIn {
    let exchange_url = url::Url::parse(exchange_url).expect("parsed exchange URL");
    let exchange_path = path_and_query(&exchange_url);
    let wrong_host = browser_request(
        &session.fixture,
        "GET",
        &exchange_path,
        &format!("{}:8787", session.deployment_host),
        &[],
        "",
        None,
    )
    .await;
    assert_eq!(wrong_host.status(), StatusCode::FOUND);
    assert_identity_login_location(wrong_host.headers());
    let exchange = browser_request(
        &session.fixture,
        "GET",
        &exchange_path,
        &format!("{}:8787", session.host),
        &[],
        "",
        None,
    )
    .await;
    assert_eq!(exchange.status(), StatusCode::SEE_OTHER);
    assert_eq!(exchange.headers()[header::LOCATION], return_path);
    let cookie = cookie_from(&exchange);
    SignedIn {
        cookie,
        exchange_path,
    }
}

async fn challenge_continuation(session: &SessionFixture) -> String {
    let challenge = browser_request(
        &session.fixture,
        "GET",
        "/",
        &format!("{}:8787", session.host),
        &[("accept", "text/html")],
        "",
        None,
    )
    .await;
    assert_eq!(challenge.status(), StatusCode::FOUND);
    continuation_from(&challenge)
}

async fn submit_login(
    session: &SessionFixture,
    continuation: &str,
    login_key: &str,
    host: &str,
) -> Response {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("continuation", continuation)
        .append_pair("login_key", login_key)
        .finish();
    browser_request(
        &session.fixture,
        "POST",
        "/account/yard-login",
        host,
        &[("content-type", "application/x-www-form-urlencoded")],
        &body,
        None,
    )
    .await
}

fn continuation_from(response: &Response) -> String {
    let login_url = response.headers()[header::LOCATION]
        .to_str()
        .expect("login URL");
    url::Url::parse(login_url)
        .expect("parsed login URL")
        .query_pairs()
        .find_map(|(name, value)| (name == "continuation").then(|| value.into_owned()))
        .expect("continuation")
}

fn cookie_from(response: &Response) -> String {
    response.headers()[header::SET_COOKIE]
        .to_str()
        .expect("session cookie")
        .split(';')
        .next()
        .expect("cookie pair")
        .to_owned()
}

async fn private_get(session: &SessionFixture, cookie: &str, accept_html: bool) -> Response {
    let mut headers = Vec::new();
    if accept_html {
        headers.push(("accept", "text/html"));
    }
    browser_request(
        &session.fixture,
        "GET",
        "/docs/?q=one",
        &format!("{}:8787", session.host),
        &headers,
        "",
        Some(cookie),
    )
    .await
}

async fn assert_list_and_management_revoke(session: &SessionFixture, cookie: &str) {
    let listed = response_json(
        send(
            &session.fixture,
            "GET",
            &format!("/v1/yards/sessions?yardId={}", session.yard_id),
            b"",
            false,
        )
        .await,
    )
    .await;
    let sessions = listed["data"]["sessions"].as_array().expect("sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["status"], "active");
    assert_eq!(sessions[0]["userId"], session.user_id);
    assert_eq!(sessions[0]["yardId"], session.yard_id);
    assert_eq!(
        sessions[0]["hostLabel"],
        session.host.strip_suffix(".localhost").expect("host label")
    );
    assert!(sessions[0]["lastUsedAt"].is_string());
    let session_id = sessions[0]["id"].as_str().expect("session ID");
    revoke_management_session(session, session_id).await;
    assert_eq!(
        private_get(session, cookie, true).await.status(),
        StatusCode::FOUND
    );
}

async fn revoke_management_session(session: &SessionFixture, session_id: &str) {
    let body = serde_json::to_vec(&serde_json::json!({
        "sessionId": session_id,
        "yardId": session.yard_id,
    }))
    .expect("revoke body");
    for _attempt in 0..2 {
        let response = send(
            &session.fixture,
            "POST",
            "/v1/yards/sessions/revoke",
            &body,
            false,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
}

async fn browser_request(
    fixture: &TransferFixture,
    method: &str,
    uri: &str,
    host: &str,
    headers: &[(&str, &str)],
    body: &str,
    cookie: Option<&str>,
) -> Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, host);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    fixture
        .router()
        .oneshot(builder.body(Body::from(body.to_owned())).expect("request"))
        .await
        .expect("response")
}

async fn body(response: Response) -> axum::body::Bytes {
    response
        .into_body()
        .collect()
        .await
        .expect("response body")
        .to_bytes()
}

fn path_and_query(url: &url::Url) -> String {
    url.query().map_or_else(
        || url.path().to_owned(),
        |query| format!("{}?{query}", url.path()),
    )
}

fn assert_identity_login_location(headers: &HeaderMap) {
    let location = headers[header::LOCATION].to_str().expect("location");
    assert!(location.starts_with("http://127.0.0.1:8787/account/yard-login?continuation=byc_"));
}

fn uniform_failure_headers(response: &Response) -> Vec<(String, String)> {
    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                value.to_str().expect("header value").to_owned(),
            )
        })
        .collect()
}
