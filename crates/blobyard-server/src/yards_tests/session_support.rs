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

pub(super) struct SessionFixture {
    pub(super) deployment_host: String,
    pub(super) fixture: TransferFixture,
    pub(super) host: String,
    pub(super) yard_id: String,
    pub(super) login_key: String,
    pub(super) user_id: String,
}

pub(super) struct SignedIn {
    pub(super) cookie: String,
    pub(super) exchange_path: String,
}

pub(super) async fn setup() -> SessionFixture {
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

pub(super) async fn sign_in(session: &SessionFixture, return_path: &str) -> SignedIn {
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
    SignedIn {
        cookie: cookie_from(&exchange),
        exchange_path,
    }
}

pub(super) async fn challenge_continuation(session: &SessionFixture) -> String {
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

pub(super) async fn submit_login(
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

pub(super) async fn private_get(
    session: &SessionFixture,
    cookie: &str,
    accept_html: bool,
) -> Response {
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

pub(super) async fn browser_request(
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

pub(super) async fn body(response: Response) -> axum::body::Bytes {
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

pub(super) fn assert_identity_login_location(headers: &HeaderMap) {
    let location = headers[header::LOCATION].to_str().expect("location");
    assert!(location.starts_with("http://127.0.0.1:8787/account/yard-login?continuation=byc_"));
}

pub(super) fn uniform_failure_headers(response: &Response) -> Vec<(String, String)> {
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
