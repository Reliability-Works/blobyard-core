#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    test_support::{
        CLIENT_SECRET, NONCE, base_claims, provider, provider_without_token_endpoint, token,
    },
    verified_identity,
};
use crate::yard_oidc_provider::{YardOidcProvider, YardOidcProviderError};
use blobyard_core::SecretString;
use openidconnect::core::CoreTokenResponse;

fn secret(value: &str) -> SecretString {
    SecretString::new(value).expect("non-empty secret")
}

#[tokio::test]
async fn exchange_rejects_a_discovered_client_without_a_token_endpoint() {
    assert_eq!(
        provider_without_token_endpoint()
            .exchange(secret("code"), secret(NONCE), secret("verifier"), 1)
            .await
            .err(),
        Some(YardOidcProviderError::InvalidResponse)
    );
}

#[tokio::test]
async fn verified_identity_propagates_an_invalid_authorized_party() {
    let provider = provider();
    let mut payload = base_claims();
    payload["azp"] = serde_json::json!("other");
    let id_token = token(&payload, CLIENT_SECRET);
    let response: CoreTokenResponse = serde_json::from_value(serde_json::json!({
        "access_token": "provider-access-value",
        "token_type": "Bearer",
        "id_token": id_token.to_string()
    }))
    .expect("token response");

    assert_eq!(
        verified_identity(
            &provider.client,
            &provider.http,
            &provider.issuer,
            &provider.client_id,
            &response,
            &secret(NONCE),
            u64::MAX,
        )
        .await
        .err(),
        Some(YardOidcProviderError::InvalidResponse)
    );
}
