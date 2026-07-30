#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{OidcHttpClient, RemoteYardOidcProvider};
use crate::yard_oidc_provider::{YardOidcAuthorization, YardOidcProvider};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use blobyard_core::SecretString;
use hmac::{Hmac, Mac};
use openidconnect::{
    ClientId, ClientSecret, IssuerUrl, RedirectUrl,
    core::{CoreClient, CoreIdToken, CoreIdTokenVerifier, CoreJsonWebKeySet, CoreProviderMetadata},
};
use sha2::Sha256;
use std::{collections::BTreeSet, time::SystemTime};

pub(super) const ISSUER: &str = "https://identity.example.test/";
pub(super) const CLIENT_ID: &str = "blobyard-client";
pub(super) const CLIENT_SECRET: &str = "fixture-client-secret";
pub(super) const NONCE: &str = "fixture-nonce";

pub(super) fn claims(
    audiences: &[&str],
    authorized_party: Option<&str>,
) -> openidconnect::core::CoreIdTokenClaims {
    let mut value = base_claims();
    value["aud"] = serde_json::json!(audiences);
    if let Some(authorized_party) = authorized_party {
        value["azp"] = serde_json::json!(authorized_party);
    }
    serde_json::from_value(value).expect("claims")
}

pub(super) fn base_claims() -> serde_json::Value {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("current time")
        .as_secs();
    serde_json::json!({
        "iss": ISSUER,
        "aud": [CLIENT_ID],
        "exp": now + 300,
        "iat": now,
        "nonce": NONCE,
        "sub": "provider-subject"
    })
}

pub(super) fn token(payload: &serde_json::Value, secret: &str) -> CoreIdToken {
    serde_json::from_value(serde_json::Value::String(raw_token(payload, secret))).expect("ID token")
}

pub(super) fn raw_token(payload: &serde_json::Value, secret: &str) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256"}"#);
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).expect("payload"));
    let signing_input = format!("{header}.{payload}");
    let mut signer = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC fixture key");
    signer.update(signing_input.as_bytes());
    let signature = URL_SAFE_NO_PAD.encode(signer.finalize().into_bytes());
    format!("{signing_input}.{signature}")
}

pub(super) fn verifier(secret: &str) -> CoreIdTokenVerifier<'static> {
    CoreIdTokenVerifier::new_confidential_client(
        ClientId::new(CLIENT_ID.to_owned()),
        ClientSecret::new(secret.to_owned()),
        IssuerUrl::new(ISSUER.to_owned()).expect("issuer"),
        CoreJsonWebKeySet::new(Vec::new()),
    )
    .allow_any_alg()
}

pub(super) fn provider() -> RemoteYardOidcProvider {
    provider_with_metadata(serde_json::json!({
        "issuer": ISSUER,
        "authorization_endpoint": "https://identity.example.test/authorize",
        "token_endpoint": "https://identity.example.test/token",
        "userinfo_endpoint": "https://identity.example.test/userinfo",
        "jwks_uri": "https://identity.example.test/keys",
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["HS256"]
    }))
}

pub(super) fn provider_without_token_endpoint() -> RemoteYardOidcProvider {
    provider_with_metadata(serde_json::json!({
        "issuer": ISSUER,
        "authorization_endpoint": "https://identity.example.test/authorize",
        "userinfo_endpoint": "https://identity.example.test/userinfo",
        "jwks_uri": "https://identity.example.test/keys",
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["HS256"]
    }))
}

fn provider_with_metadata(value: serde_json::Value) -> RemoteYardOidcProvider {
    let metadata: CoreProviderMetadata = serde_json::from_value(value).expect("provider metadata");
    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(CLIENT_ID.to_owned()),
        Some(ClientSecret::new(CLIENT_SECRET.to_owned())),
    )
    .set_redirect_uri(
        RedirectUrl::new("https://core.example.test/account/yard-oidc/callback".to_owned())
            .expect("redirect"),
    );
    RemoteYardOidcProvider {
        client,
        http: OidcHttpClient::new().expect("HTTP client"),
        issuer: ISSUER.to_owned(),
        client_id: CLIENT_ID.to_owned(),
    }
}

pub(super) fn assert_authorization_contract() {
    let url = provider()
        .authorization_url(&YardOidcAuthorization {
            state: SecretString::new("byos_fixture-state").expect("state"),
            nonce: SecretString::new(NONCE).expect("nonce"),
            pkce_verifier: SecretString::new("v".repeat(64)).expect("verifier"),
        })
        .expect("authorization URL");
    let parsed = url::Url::parse(&url).expect("authorization URL");
    let pairs = parsed.query_pairs().collect::<Vec<_>>();
    let value = |key| {
        pairs
            .iter()
            .find(|(candidate, _value)| candidate == key)
            .map(|(_key, value)| value.as_ref())
            .expect("query field")
    };
    assert_eq!(value("response_type"), "code");
    assert_eq!(value("client_id"), CLIENT_ID);
    assert_eq!(
        value("redirect_uri"),
        "https://core.example.test/account/yard-oidc/callback"
    );
    assert_eq!(value("state"), "byos_fixture-state");
    assert_eq!(value("nonce"), NONCE);
    assert_eq!(value("code_challenge_method"), "S256");
    assert_eq!(value("code_challenge").len(), 43);
    let scopes = value("scope").split(' ').collect::<BTreeSet<_>>();
    assert_eq!(scopes, BTreeSet::from(["email", "openid", "profile"]));
    assert!(!pairs.iter().any(|(key, _value)| key == "code_verifier"));
}
