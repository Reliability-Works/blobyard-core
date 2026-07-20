//! Loopback-only reqwest transport contract tests.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use blobyard_api_client::{
    ApiClient, ApiClientConfig, ApiRequest, EmptyResponse, Endpoint, ReqwestTransport, RetryAdvice,
    Transport,
};
use blobyard_core::{ErrorCode, SecretString};
use std::fmt::Write as _;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn server(response: Vec<u8>) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let read = socket.read(&mut buffer).await.expect("request read");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if complete_request(&request) {
                break;
            }
        }
        socket.write_all(&response).await.expect("response write");
        socket.shutdown().await.expect("shutdown");
        request
    });
    (format!("http://{address}/v1"), task)
}

fn complete_request(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let content_length = headers.lines().find_map(|line| {
        line.strip_prefix("content-length: ")
            .or_else(|| line.strip_prefix("Content-Length: "))
            .and_then(|value| value.parse::<usize>().ok())
    });
    content_length.is_none_or(|length| request.len() >= header_end + 4 + length)
}

fn http_response(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut response = format!("HTTP/1.1 {status}\r\nConnection: close\r\n");
    for (name, value) in headers {
        let _ = write!(response, "{name}: {value}\r\n");
    }
    response.push_str("\r\n");
    let mut bytes = response.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

#[tokio::test]
async fn sends_one_redaction_safe_prepared_request() {
    let body = br#"{"ok":true,"data":{},"requestId":"req_transport"}"#;
    let length = body.len().to_string();
    let response = http_response(
        "200 OK",
        &[
            ("X-Request-Id", "req_transport"),
            ("Content-Length", &length),
        ],
        body,
    );
    let (base, captured) = server(response).await;
    let transport =
        ReqwestTransport::new(ApiClientConfig::new(base).expect("config")).expect("transport");
    let request = ApiRequest::new(Endpoint::RequestAccountExport)
        .with_json(serde_json::json!({}))
        .with_bearer(SecretString::new("bearer-secret").expect("secret"))
        .with_idempotency_key("stable-key".into())
        .expect("key");
    let success = ApiClient::new(std::sync::Arc::new(transport))
        .execute::<EmptyResponse>(request)
        .await
        .expect("success");
    assert_eq!(success.request_id(), "req_transport");
    let request_bytes = captured.await.expect("server task");
    let request = String::from_utf8_lossy(&request_bytes);
    assert!(request.starts_with("POST /v1/account/exports HTTP/1.1"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer bearer-secret")
    );
    assert!(
        request
            .to_ascii_lowercase()
            .contains("idempotency-key: stable-key")
    );
    assert!(request.contains("{}"));
}

#[tokio::test]
async fn parses_retry_after_without_retrying() {
    let body =
        br#"{"ok":false,"error":{"code":"RATE_LIMITED","message":"wait"},"requestId":"req_rate"}"#;
    let response = http_response(
        "429 Too Many Requests",
        &[("X-Request-Id", "req_rate"), ("Retry-After", "6")],
        body,
    );
    let (base, captured) = server(response).await;
    let transport =
        ReqwestTransport::new(ApiClientConfig::new(base).expect("config")).expect("transport");
    let error = ApiClient::new(std::sync::Arc::new(transport))
        .execute::<EmptyResponse>(ApiRequest::new(Endpoint::Health))
        .await
        .expect_err("rate limit");
    assert_eq!(error.error().code(), ErrorCode::RateLimited);
    assert_eq!(
        error.retry_advice(),
        RetryAdvice::After(Duration::from_secs(6))
    );
    captured.await.expect("server task");
}

#[tokio::test]
async fn rejects_declared_and_streamed_oversized_responses() {
    let declared = http_response(
        "200 OK",
        &[("X-Request-Id", "req_large"), ("Content-Length", "1048577")],
        b"",
    );
    let (base, captured) = server(declared).await;
    let transport =
        ReqwestTransport::new(ApiClientConfig::new(base).expect("config")).expect("transport");
    let error = transport
        .send(&ApiRequest::new(Endpoint::Health))
        .await
        .expect_err("declared size");
    assert_eq!(error.error().code(), ErrorCode::ProviderUnavailable);
    captured.await.expect("server task");

    let streamed = http_response(
        "200 OK",
        &[("X-Request-Id", "req_stream")],
        &vec![b'x'; 1_048_577],
    );
    let (base, captured) = server(streamed).await;
    let transport =
        ReqwestTransport::new(ApiClientConfig::new(base).expect("config")).expect("transport");
    let error = transport
        .send(&ApiRequest::new(Endpoint::Health))
        .await
        .expect_err("streamed size");
    assert_eq!(error.error().code(), ErrorCode::ProviderUnavailable);
    captured.await.expect("server task");
}

#[tokio::test]
async fn classifies_connection_and_truncated_body_failures() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    drop(listener);
    let config = ApiClientConfig::new(format!("http://{address}/v1")).expect("config");
    let transport = ReqwestTransport::new(config).expect("transport");
    let error = transport
        .send(&ApiRequest::new(Endpoint::Health))
        .await
        .expect_err("connection failure");
    assert_eq!(error.error().code(), ErrorCode::NetworkError);
    assert_eq!(error.retry_advice(), RetryAdvice::SafeRequest);

    let truncated = http_response(
        "200 OK",
        &[("X-Request-Id", "req_short"), ("Content-Length", "20")],
        b"{}",
    );
    let (base, captured) = server(truncated).await;
    let transport =
        ReqwestTransport::new(ApiClientConfig::new(base).expect("config")).expect("transport");
    let error = transport
        .send(&ApiRequest::new(Endpoint::CreateProject))
        .await
        .expect_err("truncated body");
    assert_eq!(error.error().code(), ErrorCode::NetworkError);
    assert_eq!(error.retry_advice(), RetryAdvice::Never);
    captured.await.expect("server task");
}
