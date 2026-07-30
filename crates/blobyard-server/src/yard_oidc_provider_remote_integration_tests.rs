#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    RemoteYardOidcProvider,
    integration_test_support::{
        CLIENT_ID, CLIENT_SECRET, LoopbackProvider, NONCE, PUBLIC_ORIGIN, VERIFIER, request,
    },
};
use crate::{
    ServerError, YardOidcConfiguration,
    yard_oidc_provider::{
        self, YardOidcProvider, YardOidcProviderError, YardOidcVerifiedIdentity,
        http::{INSECURE_URL_MESSAGE, OidcHttpClient},
    },
};
use axum::http::StatusCode;
use blobyard_core::SecretString;
use openidconnect::{AsyncHttpClient, HttpClientError};
use std::time::SystemTime;
use tokio::io::AsyncWriteExt;

fn secret(value: impl Into<String>) -> SecretString {
    SecretString::new(value).expect("non-empty secret")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("current time")
        .as_millis()
        .try_into()
        .expect("milliseconds fit")
}

async fn exchange(
    provider: &dyn YardOidcProvider,
    code: &str,
) -> Result<YardOidcVerifiedIdentity, YardOidcProviderError> {
    provider
        .exchange(secret(code), secret(NONCE), secret(VERIFIER), now_ms())
        .await
}

#[tokio::test]
async fn loopback_discovery_and_exchange_cover_success_and_provider_failures() {
    let fixture = LoopbackProvider::start().await;
    let configuration = fixture.configuration();
    let provider = RemoteYardOidcProvider::discover(&configuration, PUBLIC_ORIGIN)
        .await
        .expect("provider discovery");

    let claims_identity = exchange(&provider, "claims-email")
        .await
        .expect("claims identity");
    assert_eq!(claims_identity.issuer, fixture.issuer);
    assert_eq!(claims_identity.provider_subject, "provider-subject");
    assert_eq!(
        claims_identity.normalized_email.as_deref(),
        Some("person@example.test")
    );

    let user_info_identity = exchange(&provider, "userinfo")
        .await
        .expect("user info identity");
    assert_eq!(
        user_info_identity.normalized_email.as_deref(),
        Some("userinfo@example.test")
    );

    for code in [
        "missing-id-token",
        "bad-signature",
        "invalid-subject",
        "future-not-before",
        "wrong-hash",
        "wrong-authorized-party",
        "userinfo-failure",
        "userinfo-foreign-token",
    ] {
        assert_eq!(
            exchange(&provider, code).await.err(),
            Some(YardOidcProviderError::InvalidResponse),
            "{code}"
        );
    }
    assert_eq!(
        exchange(&provider, "unavailable").await.err(),
        Some(YardOidcProviderError::Unavailable)
    );
    assert_eq!(
        exchange(&provider, "oversized").await.err(),
        Some(YardOidcProviderError::Unavailable)
    );
    assert_eq!(
        provider
            .exchange(
                secret("claims-email"),
                secret("wrong-nonce"),
                secret(VERIFIER),
                now_ms(),
            )
            .await
            .err(),
        Some(YardOidcProviderError::InvalidResponse)
    );
}

#[tokio::test]
async fn exchange_fails_closed_without_a_user_info_endpoint() {
    let no_user_info_fixture = LoopbackProvider::start().await;
    no_user_info_fixture.metadata_mode(5);
    let no_user_info =
        RemoteYardOidcProvider::discover(&no_user_info_fixture.configuration(), PUBLIC_ORIGIN)
            .await
            .expect("provider without user info endpoint");
    assert_eq!(
        exchange(&no_user_info, "userinfo").await.err(),
        Some(YardOidcProviderError::InvalidResponse)
    );
}

#[tokio::test]
async fn configuration_discovery_fails_closed_for_metadata_and_network_errors() {
    let fixture = LoopbackProvider::start().await;
    let configuration = fixture.configuration();
    assert!(
        yard_oidc_provider::configured(Some(&configuration), PUBLIC_ORIGIN)
            .await
            .expect("configured provider")
            .is_some()
    );
    assert_eq!(
        RemoteYardOidcProvider::discover(&configuration, "not a public origin")
            .await
            .err(),
        Some(ServerError::PublicOrigin)
    );
    for mode in [1, 2, 3, 4] {
        fixture.metadata_mode(mode);
        if mode == 1 {
            assert_eq!(
                yard_oidc_provider::configured(Some(&configuration), PUBLIC_ORIGIN)
                    .await
                    .err(),
                Some(ServerError::OidcDiscovery)
            );
        }
        assert_eq!(
            RemoteYardOidcProvider::discover(&configuration, PUBLIC_ORIGIN)
                .await
                .err(),
            Some(ServerError::OidcDiscovery),
            "metadata mode {mode}"
        );
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("unused address listener");
    let unavailable_issuer = format!("http://{}/", listener.local_addr().expect("unused address"));
    drop(listener);
    let unavailable = YardOidcConfiguration::from_optional(
        Some(unavailable_issuer),
        Some(CLIENT_ID.to_owned()),
        SecretString::new(CLIENT_SECRET).ok(),
    )
    .expect("configuration")
    .expect("enabled");
    assert_eq!(
        RemoteYardOidcProvider::discover(&unavailable, PUBLIC_ORIGIN)
            .await
            .err(),
        Some(ServerError::OidcDiscovery)
    );
}

#[tokio::test]
async fn configuration_helpers_fail_closed_without_an_http_client() {
    let fixture = LoopbackProvider::start().await;
    assert_eq!(
        RemoteYardOidcProvider::discover_with_http(
            &fixture.configuration(),
            PUBLIC_ORIGIN,
            Err(ServerError::OidcDiscovery),
        )
        .await
        .err(),
        Some(ServerError::OidcDiscovery)
    );
    assert!(
        yard_oidc_provider::configured(None, PUBLIC_ORIGIN)
            .await
            .expect("disabled provider")
            .is_none()
    );
}

#[tokio::test]
async fn oidc_http_client_conceals_conversion_network_and_body_errors() {
    let fixture = LoopbackProvider::start().await;
    let client = OidcHttpClient::new().expect("OIDC HTTP client");
    let response = client
        .call(request(&format!("{}keys", fixture.issuer)))
        .await
        .expect("successful request");
    assert_eq!(response.status(), StatusCode::OK);

    assert!(client.call(request("/relative")).await.is_err());
    for insecure in [
        "http://identity.example.test/keys",
        "http://user@127.0.0.1:9000/keys",
    ] {
        assert!(
            matches!(
                client.call(request(insecure)).await.err(),
                Some(HttpClientError::Other(message)) if message == INSECURE_URL_MESSAGE
            ),
            "{insecure}"
        );
    }

    let unused = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("unused listener");
    let unused_url = format!(
        "http://{}/unavailable",
        unused.local_addr().expect("unused address")
    );
    drop(unused);
    assert!(client.call(request(&unused_url)).await.is_err());

    let incomplete = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("incomplete body listener");
    let incomplete_url = format!(
        "http://{}/incomplete",
        incomplete.local_addr().expect("incomplete address")
    );
    let task = tokio::spawn(async move {
        let (mut stream, _peer) = incomplete.accept().await.expect("accepted request");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nshort")
            .await
            .expect("partial response");
    });
    assert!(client.call(request(&incomplete_url)).await.is_err());
    task.await.expect("partial response task");
}
