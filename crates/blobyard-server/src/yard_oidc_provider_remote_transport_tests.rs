#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    RemoteYardOidcProvider,
    integration_test_support::{
        CLIENT_ID, CLIENT_SECRET, LoopbackProvider, PUBLIC_ORIGIN, request,
    },
};
use crate::{
    ServerError, YardOidcConfiguration,
    yard_oidc_provider::http::{OVERSIZED_MESSAGE, OidcHttpClient},
};
use axum::http::StatusCode;
use blobyard_core::SecretString;
use openidconnect::{AsyncHttpClient, HttpClientError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn loopback_token_endpoint_enforces_the_exact_token_request() {
    let fixture = LoopbackProvider::start().await;
    let response = reqwest::Client::new()
        .post(format!("{}token", fixture.issuer))
        .header(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body("grant_type=client_credentials&code=claims-email")
        .send()
        .await
        .expect("token endpoint response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn callback_transport_is_validated_before_provider_discovery() {
    let fixture = LoopbackProvider::start().await;
    fixture.metadata_mode(4);
    assert_eq!(
        RemoteYardOidcProvider::discover(&fixture.configuration(), "http://core.example.test")
            .await
            .err(),
        Some(ServerError::PublicOrigin),
        "an insecure callback origin fails before any provider discovery"
    );
}

#[tokio::test]
async fn discovery_rejects_an_insecure_jwks_url_without_requesting_it() {
    let fixture = LoopbackProvider::start().await;
    fixture.metadata_mode(6);
    assert_eq!(
        RemoteYardOidcProvider::discover(&fixture.configuration(), PUBLIC_ORIGIN)
            .await
            .err(),
        Some(ServerError::OidcDiscovery)
    );
    assert_eq!(
        fixture.keys_request_count(),
        0,
        "the insecure JWKS URL must receive no request"
    );
}

#[tokio::test]
async fn discovery_fails_closed_for_an_oversized_response() {
    let url = raw_response_listener(
        "HTTP/1.1 200 OK\r\nContent-Length: 4194305\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    let configuration = YardOidcConfiguration::from_optional(
        Some(format!("{url}/")),
        Some(CLIENT_ID.to_owned()),
        SecretString::new(CLIENT_SECRET).ok(),
    )
    .expect("configuration")
    .expect("enabled");
    assert_eq!(
        RemoteYardOidcProvider::discover(&configuration, PUBLIC_ORIGIN)
            .await
            .err(),
        Some(ServerError::OidcDiscovery)
    );
}

#[tokio::test]
async fn oidc_http_client_bounds_declared_and_streamed_response_bodies() {
    let client = OidcHttpClient::new().expect("OIDC HTTP client");
    let declared = raw_response_listener(
        "HTTP/1.1 200 OK\r\nContent-Length: 4194305\r\n\r\n".to_owned(),
        Vec::new(),
    )
    .await;
    assert_bounded(declared, &client, "a declared length over the limit").await;

    let mut payload = Vec::new();
    for _ in 0..65 {
        payload.extend_from_slice(b"10000\r\n");
        payload.extend(std::iter::repeat_n(b'x', 65_536));
        payload.extend_from_slice(b"\r\n");
    }
    payload.extend_from_slice(b"0\r\n\r\n");
    let chunked = raw_response_listener(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_owned(),
        payload,
    )
    .await;
    assert_bounded(chunked, &client, "a streamed body over the limit").await;
}

async fn assert_bounded(url: String, client: &OidcHttpClient, case: &str) {
    let outcome = client.call(request(&url)).await;
    assert!(
        matches!(
            outcome.as_ref().err(),
            Some(HttpClientError::Other(message)) if message.as_str() == OVERSIZED_MESSAGE
        ),
        "{case} is rejected: {outcome:?}"
    );
}

async fn raw_response_listener(head: String, body: Vec<u8>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("raw response listener");
    let url = format!("http://{}", listener.local_addr().expect("raw address"));
    tokio::spawn(async move {
        let (mut stream, _peer) = listener.accept().await.expect("accepted request");
        let mut request = [0_u8; 1_024];
        let _request = stream.read(&mut request).await;
        let _head = stream.write_all(head.as_bytes()).await;
        for piece in body.chunks(16_384) {
            if stream.write_all(piece).await.is_err() {
                return;
            }
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    });
    url
}
