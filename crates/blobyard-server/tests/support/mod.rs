use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use blobyard_server::initialize;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

/// Authorized temporary standalone server.
pub struct AuthorizedServer {
    /// Durable temporary data directory.
    pub temporary: tempfile::TempDir,
    /// In-process HTTP router.
    pub router: Router,
    /// Exchanged local operator token.
    pub access_token: String,
    /// Consumed first-start bootstrap token.
    pub bootstrap_token: String,
}

/// Initializes a temporary server and exchanges its bootstrap token.
///
/// # Panics
///
/// Panics when the isolated fixture cannot initialize or exchange its token.
pub async fn authorized_server() -> AuthorizedServer {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let mut initialized = initialize(temporary.path()).expect("initialized server");
    let bootstrap = initialized
        .take_bootstrap_token()
        .expect("first-start bootstrap token");
    let router = initialized.router();
    let exchange = send(
        &router,
        "POST",
        "/v1/bootstrap/exchange",
        Some(json!({
            "name": "Local operator",
            "platform": "test",
            "token": bootstrap.expose_secret(),
            "version": "0.0.0-test"
        })),
        None,
    )
    .await;
    assert_eq!(exchange.0, StatusCode::OK);
    assert_eq!(exchange.1["data"]["webYardOrigin"], "http://localhost:8787");
    let access_token = exchange.1["data"]["accessToken"]
        .as_str()
        .expect("access token")
        .to_owned();
    assert!(!access_token.contains(bootstrap.expose_secret()));
    AuthorizedServer {
        temporary,
        router,
        access_token,
        bootstrap_token: bootstrap.expose_secret().to_owned(),
    }
}

/// Creates the standard `fixture` project used by transfer-edge tests.
///
/// # Panics
///
/// Panics when the authenticated project request does not succeed.
pub async fn create_fixture_project(server: &AuthorizedServer) {
    let project = send(
        &server.router,
        "POST",
        "/v1/projects",
        Some(json!({ "workspace": "default", "name": "Fixture" })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(project.0, StatusCode::OK);
}

/// Reserves a five-byte text upload in the standard `fixture` project.
///
/// # Panics
///
/// Panics when the authenticated upload reservation does not succeed.
pub async fn reserve_fixture_upload(
    server: &AuthorizedServer,
    idempotency: &str,
    path: &str,
) -> Value {
    let response = send_idempotent(
        &server.router,
        "POST",
        "/v1/uploads/request",
        Some(json!({
            "workspace": "default",
            "project": "fixture",
            "path": path,
            "filename": "hello.txt",
            "sizeBytes": 5,
            "checksumSha256": "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            "contentType": "text/plain"
        })),
        Some(&server.access_token),
        Some(idempotency),
    )
    .await;
    assert_eq!(response.0, StatusCode::OK);
    response.1
}

/// Uploads the standard five-byte fixture body through an existing reservation.
///
/// # Panics
///
/// Panics when the fixture transfer cannot be served or does not succeed.
pub async fn upload_fixture_bytes(server: &AuthorizedServer, reservation: &Value) {
    let uploaded = send_bytes(
        &server.router,
        "PUT",
        &transfer_path(reservation, "uploadUrl"),
        b"hello".to_vec(),
        None,
        None,
        Some("text/plain"),
    )
    .await;
    assert_eq!(uploaded.0, StatusCode::NO_CONTENT);
}

/// Uploads and completes an existing fixture reservation.
///
/// # Panics
///
/// Panics when the reservation is malformed or the transfer cannot be completed.
pub async fn complete_fixture_upload(server: &AuthorizedServer, reservation: &Value) -> Value {
    upload_fixture_bytes(server, reservation).await;
    let upload_id = reservation["data"]["uploadId"].as_str().expect("upload ID");
    let completed = send(
        &server.router,
        "POST",
        "/v1/uploads/complete",
        Some(json!({ "uploadId": upload_id, "parts": [] })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(completed.0, StatusCode::OK);
    completed.1
}

/// Verifies the stable public envelope for an internal server failure.
///
/// # Panics
///
/// Panics when the response does not match the public internal-error contract.
pub fn assert_internal(status: StatusCode, body: &Value) {
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "INTERNAL_ERROR");
    assert_eq!(
        body["error"]["message"],
        "Blobyard couldn't complete that. Try again or contact support."
    );
    assert!(
        body["requestId"]
            .as_str()
            .is_some_and(|request_id| request_id.starts_with("req_"))
    );
}

/// Sends a JSON request and parses its JSON response.
pub async fn send(
    router: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    send_idempotent(router, method, uri, body, token, None).await
}

/// Sends a JSON request with an optional idempotency key.
///
/// # Panics
///
/// Panics when fixture JSON cannot be encoded or the response is not JSON.
pub async fn send_idempotent(
    router: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
    idempotency_key: Option<&str>,
) -> (StatusCode, Value) {
    let body = body.map_or_else(Vec::new, |value| serde_json::to_vec(&value).expect("JSON"));
    let (status, bytes) = send_bytes(
        router,
        method,
        uri,
        body,
        token,
        idempotency_key,
        Some("application/json"),
    )
    .await;
    (
        status,
        serde_json::from_slice(&bytes).expect("response JSON"),
    )
}

/// Sends caller-provided JSON bytes and parses the response.
///
/// # Panics
///
/// Panics when the fixture response is not JSON.
pub async fn send_json_bytes(
    router: &Router,
    method: &str,
    uri: &str,
    body: Vec<u8>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let (status, bytes) = send_bytes(
        router,
        method,
        uri,
        body,
        token,
        None,
        Some("application/json"),
    )
    .await;
    (
        status,
        serde_json::from_slice(&bytes).expect("response JSON"),
    )
}

/// Sends arbitrary bytes and returns the exact response body.
///
/// # Panics
///
/// Panics when the in-process request cannot be constructed or served.
pub async fn send_bytes(
    router: &Router,
    method: &str,
    uri: &str,
    body: Vec<u8>,
    token: Option<&str>,
    idempotency_key: Option<&str>,
    content_type: Option<&str>,
) -> (StatusCode, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(content_type) = content_type {
        request = request.header(header::CONTENT_TYPE, content_type);
    }
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(value) = idempotency_key {
        request = request.header("idempotency-key", value);
    }
    let (status, _headers, bytes) =
        serve(router, request.body(Body::from(body)).expect("request")).await;
    (status, bytes)
}

/// Sends an unauthenticated ranged GET and returns response headers and bytes.
///
/// # Panics
///
/// Panics when the in-process request cannot be constructed or served.
pub async fn send_range(
    router: &Router,
    uri: &str,
    range: Option<&str>,
) -> (StatusCode, header::HeaderMap, Vec<u8>) {
    let mut request = Request::builder().method("GET").uri(uri);
    if let Some(value) = range {
        request = request.header(header::RANGE, value);
    }
    serve(router, request.body(Body::empty()).expect("request")).await
}

async fn serve(
    router: &Router,
    request: Request<Body>,
) -> (StatusCode, header::HeaderMap, Vec<u8>) {
    let response = router.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("response body")
        .to_bytes()
        .to_vec();
    (status, headers, bytes)
}

/// Extracts the path component from a transfer URL in a success envelope.
///
/// # Panics
///
/// Panics when the fixture response does not contain a valid transfer URL.
#[must_use]
pub fn transfer_path(response: &Value, field: &str) -> String {
    let url = response["data"][field].as_str().expect("transfer URL");
    url::Url::parse(url).expect("parsed URL").path().to_owned()
}

/// Asserts one authenticated upload-status response.
///
/// # Panics
///
/// Panics when the fixture does not return the expected successful state.
pub async fn assert_upload_status(router: &Router, token: &str, upload_id: &str, expected: &str) {
    let status = send(
        router,
        "GET",
        &format!("/v1/uploads/status?uploadId={upload_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status.0, StatusCode::OK);
    assert_eq!(status.1["data"]["state"], expected);
}

/// Completes one single-part upload through the authenticated HTTP route.
pub async fn complete_single_upload(
    router: &Router,
    token: &str,
    upload_id: &str,
) -> (StatusCode, Value) {
    send(
        router,
        "POST",
        "/v1/uploads/complete",
        Some(json!({ "uploadId": upload_id, "parts": [] })),
        Some(token),
    )
    .await
}
