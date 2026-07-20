//! Normal-library coverage for the remote GitHub OIDC verifier.

#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use blobyard_server::oidc::{
    GithubOidcVerifier, OidcVerificationError, RemoteGithubOidcVerifier, verify_test_claims,
};
use serde_json::{Value, json};
use tokio::net::TcpListener;

#[path = "support/oidc.rs"]
mod server;
use server::{MockJwksServer, chunked_response, declared_length_response, response};

const NOW_MS: u64 = 1_800_000_000_000;
const AUDIENCE: &str = "https://api.blobyard.local";
const CACHE_TTL_MS: u64 = 10 * 60 * 1_000;
const COOLDOWN_MS: u64 = 30 * 1_000;
const MAX_JWKS_BYTES: usize = 256 * 1_024;
const SIGNED_TOKEN: &str = concat!(
    "eyJhbGciOiJSUzI1NiIsImtpZCI6ImZpeHR1cmUiLCJ0eXAiOiJKV1QifQ",
    ".",
    "eyJhdWQiOiJodHRwczovL2FwaS5ibG9ieWFyZC5sb2NhbCIsImV4cCI6MTgwMDAwMDYwMCwiaWF0IjoxODAwMDAwMDAwLCJpc3MiOiJodHRwczovL3Rva2VuLmFjdGlvbnMuZ2l0aHVidXNlcmNvbnRlbnQuY29tIiwibmJmIjoxODAwMDAwMDAwLCJyZWYiOiJyZWZzL2hlYWRzL21haW4iLCJyZXBvc2l0b3J5IjoiUmVsaWFiaWxpdHktV29ya3MvQmxvYnlhcmQtQ29yZSIsInJlcG9zaXRvcnlfb3duZXIiOiJSZWxpYWJpbGl0eS1Xb3JrcyIsInJ1bl9hdHRlbXB0IjoiMSIsInJ1bl9pZCI6IjEyMzQ1Iiwic2hhIjoiYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYSIsInN1YiI6InJlcG86UmVsaWFiaWxpdHktV29ya3NAMTIzL0Jsb2J5YXJkLUNvcmVANDU2OnJlZjpyZWZzL2hlYWRzL21haW4iLCJ3b3JrZmxvd19yZWYiOiJSZWxpYWJpbGl0eS1Xb3Jrcy9CbG9ieWFyZC1Db3JlLy5naXRodWIvd29ya2Zsb3dzL3JlbGVhc2UueW1sQHJlZnMvaGVhZHMvbWFpbiJ9",
    ".",
    "EReuMrLt-uvpjq4I8X3jgacHKQwAAEnPgvSXlTYO80UaWbrWHuEk2CX3d7ws08i7yx-r0qa6_G0vsQewrnc3tyWCCm9vfx3XpAiv2Jf92tzw50JAPffOwKdUT2gGDegj7Crdfgj9gnTjtxsGY1fGdQagORxxvkofEPvdbsZ-ynoATRIwPoXGgEyl8wPpfAhUxczVs2O1kwmeei7v0-YPkXkUwK8cw-0q2XQfLe1LIjfsmOUlyCH6YsVog8b6JD3ozak8rWt4b-40K3fNYd-A58JVb2VyjOOOiMUV9kn6yGsKlA7CDg8NH45HYrguPFPXYJh7uzvK807fyByv1t8vfg"
);

fn key_value(kid: &str) -> Value {
    json!({
        "alg": "RS256",
        "e": "AQAB",
        "kid": kid,
        "kty": "RSA",
        "n": "0LJy5SQy15d12kuYvGzrJiB3fyeq4nOCVEkF-xN-cvzvsI4u8fUqu6v3zc1BWtSgBA4q7I9PJQ6qSiUnO5wL9Xh0XGUAY37ww1YdQIZaI71rgz6P9rqbFD5WUCyGxPmL0mLV56XFkyTzowA-O_iZDaIKZbTujgMws5aXnbAghWO7TMCV8VhK-SfNUIpqcaKwEFYGt7YmPprNXMg-FrwWC5TucjbSS1zHZaE7boiMrOET3lXZdMMqj58yMWWWWxvT45a0R1bMTkBAdFQtKlnIn8T1Va3rweAII7Pl8cnGFbxVnzD6MaBxbRCHS72CTcHAKULw5d0Z9kxyPlY0oi7OiQ",
        "use": "sig"
    })
}

fn jwks(key: &Value) -> Vec<u8> {
    serde_json::to_vec(&json!({ "keys": [key] })).expect("JWKS fixture")
}

fn claims() -> Value {
    json!({
        "aud": AUDIENCE,
        "exp": NOW_MS / 1_000 + 600,
        "iat": NOW_MS / 1_000,
        "iss": "https://token.actions.githubusercontent.com",
        "nbf": NOW_MS / 1_000,
        "ref": "refs/heads/main",
        "repository": "Reliability-Works/Blobyard-Core",
        "repository_owner": "Reliability-Works",
        "run_id": "12345",
        "sub": "repo:Reliability-Works@123/Blobyard-Core@456:ref:refs/heads/main",
        "workflow_ref": "Reliability-Works/Blobyard-Core/.github/workflows/release.yml@refs/heads/main"
    })
}

fn invalid_claim(value: Value) {
    assert_eq!(
        verify_test_claims(value, AUDIENCE, NOW_MS),
        Err(OidcVerificationError::Invalid)
    );
}

#[path = "oidc_verifier/claims.rs"]
mod claim_contract;

fn base64url(value: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut encoded = String::new();
    for chunk in value.chunks(3) {
        let bits = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        encoded.push(char::from(ALPHABET[((bits >> 18) & 63) as usize]));
        encoded.push(char::from(ALPHABET[((bits >> 12) & 63) as usize]));
        if chunk.len() > 1 {
            encoded.push(char::from(ALPHABET[((bits >> 6) & 63) as usize]));
        }
        if chunk.len() > 2 {
            encoded.push(char::from(ALPHABET[(bits & 63) as usize]));
        }
    }
    encoded
}

fn token_with_header(header: &Value) -> String {
    let header = serde_json::to_vec(header).expect("JWT header fixture");
    let mut parts = SIGNED_TOKEN.split('.');
    let _old_header = parts.next().expect("signed header");
    let payload = parts.next().expect("signed payload");
    let signature = parts.next().expect("signed signature");
    format!("{}.{payload}.{signature}", base64url(&header))
}

fn token_with_kid(kid: &str) -> String {
    token_with_header(&json!({ "alg": "RS256", "kid": kid, "typ": "JWT" }))
}

fn token_with_payload(payload: &str) -> String {
    let mut parts = SIGNED_TOKEN.split('.');
    let header = parts.next().expect("signed header");
    let _old_payload = parts.next().expect("signed payload");
    let signature = parts.next().expect("signed signature");
    format!("{header}.{payload}.{signature}")
}

#[tokio::test]
async fn normal_library_fetches_and_reuses_verified_keys() {
    let _default = RemoteGithubOidcVerifier::new();
    let server = MockJwksServer::start(vec![response(200, &jwks(&key_value("fixture")))]).await;
    let verifier = RemoteGithubOidcVerifier::with_test_jwks_url(server.url());
    let identity = verifier
        .verify(SIGNED_TOKEN, AUDIENCE, NOW_MS)
        .await
        .expect("verified remote token");
    assert_eq!(identity.run_id, "12345");
    assert!(
        verifier
            .verify(SIGNED_TOKEN, AUDIENCE, NOW_MS + 1)
            .await
            .is_ok()
    );
    assert_eq!(server.request_count(), 1);
}

#[tokio::test]
async fn unknown_key_refreshes_only_after_the_cooldown() {
    let server = MockJwksServer::start(vec![
        response(200, &jwks(&key_value("old"))),
        response(200, &jwks(&key_value("fixture"))),
    ])
    .await;
    let verifier = RemoteGithubOidcVerifier::with_test_jwks_url(server.url());
    assert_eq!(
        verifier.verify(SIGNED_TOKEN, AUDIENCE, NOW_MS).await,
        Err(OidcVerificationError::Invalid)
    );
    assert_eq!(server.request_count(), 1);
    assert!(
        verifier
            .verify(SIGNED_TOKEN, AUDIENCE, NOW_MS + COOLDOWN_MS + 1)
            .await
            .is_ok()
    );
    assert_eq!(server.request_count(), 2);
}

#[tokio::test]
async fn provider_failures_are_cooled_down_and_retried() {
    let server = MockJwksServer::start(vec![
        response(503, b"unavailable"),
        response(200, &jwks(&key_value("fixture"))),
    ])
    .await;
    let verifier = RemoteGithubOidcVerifier::with_test_jwks_url(server.url());
    for now_ms in [NOW_MS, NOW_MS + 1] {
        assert_eq!(
            verifier.verify(SIGNED_TOKEN, AUDIENCE, now_ms).await,
            Err(OidcVerificationError::ProviderUnavailable)
        );
    }
    assert_eq!(server.request_count(), 1);
    assert!(
        verifier
            .verify(SIGNED_TOKEN, AUDIENCE, NOW_MS + COOLDOWN_MS + 1)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn cached_keys_remain_usable_during_refresh_cooldown() {
    let server = MockJwksServer::start(vec![
        response(200, &jwks(&key_value("fixture"))),
        response(503, b"unavailable"),
        response(503, b"unavailable"),
    ])
    .await;
    let verifier = RemoteGithubOidcVerifier::with_test_jwks_url(server.url());
    assert!(
        verifier
            .verify(SIGNED_TOKEN, AUDIENCE, NOW_MS)
            .await
            .is_ok()
    );
    let missing = token_with_kid("missing");
    assert_eq!(
        verifier
            .verify(&missing, AUDIENCE, NOW_MS + COOLDOWN_MS + 1)
            .await,
        Err(OidcVerificationError::ProviderUnavailable)
    );
    assert_eq!(
        verifier
            .verify(&missing, AUDIENCE, NOW_MS + COOLDOWN_MS + 2)
            .await,
        Err(OidcVerificationError::Invalid)
    );
    assert!(
        verifier
            .verify(SIGNED_TOKEN, AUDIENCE, NOW_MS + COOLDOWN_MS + 2)
            .await
            .is_ok()
    );
    let expired = NOW_MS + CACHE_TTL_MS + COOLDOWN_MS + 1;
    assert_eq!(
        verifier.verify(SIGNED_TOKEN, AUDIENCE, expired).await,
        Err(OidcVerificationError::ProviderUnavailable)
    );
    assert_eq!(
        verifier.verify(SIGNED_TOKEN, AUDIENCE, expired + 1).await,
        Err(OidcVerificationError::ProviderUnavailable)
    );
    assert_eq!(server.request_count(), 3);
}

#[tokio::test]
async fn malformed_headers_keys_and_signatures_fail_closed() {
    let invalid_key_fixtures = [
        json!({ "alg": "RS384", "e": "AQAB", "kid": "fixture", "kty": "RSA", "n": "sXch", "use": "sig" }),
        json!({ "alg": "RS256", "e": "AQAB", "kid": "fixture", "kty": "RSA", "n": "sXch", "use": "enc" }),
        json!({ "alg": "RS256", "e": "!", "kid": "fixture", "kty": "RSA", "n": "!", "use": "sig" }),
    ];
    for key in invalid_key_fixtures {
        let server = MockJwksServer::start(vec![response(200, &jwks(&key))]).await;
        let verifier = RemoteGithubOidcVerifier::with_test_jwks_url(server.url());
        assert_eq!(
            verifier.verify(SIGNED_TOKEN, AUDIENCE, NOW_MS).await,
            Err(OidcVerificationError::Invalid)
        );
    }

    let server = MockJwksServer::start(vec![response(200, &jwks(&key_value("fixture")))]).await;
    let verifier = RemoteGithubOidcVerifier::with_test_jwks_url(server.url());
    for token in [
        "not-a-jwt".to_owned(),
        "!.e30.AA".to_owned(),
        "bm90LWpzb24.e30.AA".to_owned(),
        token_with_header(&json!({ "alg": "HS256", "kid": "fixture" })),
        token_with_header(&json!({ "alg": "RS256" })),
        token_with_payload("!"),
        token_with_payload("bm90LWpzb24"),
        token_with_payload("e30"),
        SIGNED_TOKEN.replacen("eyJhdWQi", "eyJhdWQj", 1),
    ] {
        assert_eq!(
            verifier.verify(&token, AUDIENCE, NOW_MS).await,
            Err(OidcVerificationError::Invalid)
        );
    }
}

#[tokio::test]
async fn malformed_provider_responses_are_unavailable() {
    let fixtures = [
        response(503, b"unavailable"),
        declared_length_response(MAX_JWKS_BYTES + 1, b""),
        chunked_response(&vec![b'x'; MAX_JWKS_BYTES + 1]),
        response(200, b"not-json"),
        declared_length_response(100, b"{}"),
    ];
    for fixture in fixtures {
        let server = MockJwksServer::start(vec![fixture]).await;
        let verifier = RemoteGithubOidcVerifier::with_test_jwks_url(server.url());
        assert_eq!(
            verifier.verify(SIGNED_TOKEN, AUDIENCE, NOW_MS).await,
            Err(OidcVerificationError::ProviderUnavailable)
        );
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("unused port");
    let address = listener.local_addr().expect("unused address");
    drop(listener);
    let verifier = RemoteGithubOidcVerifier::with_test_jwks_url(&format!("http://{address}/jwks"));
    assert_eq!(
        verifier.verify(SIGNED_TOKEN, AUDIENCE, NOW_MS).await,
        Err(OidcVerificationError::ProviderUnavailable)
    );
}
