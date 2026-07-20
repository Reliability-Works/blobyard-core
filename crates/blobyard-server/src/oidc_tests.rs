#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use serde_json::json;

pub(super) const SIGNED_TOKEN: &str = concat!(
    "eyJhbGciOiJSUzI1NiIsImtpZCI6ImZpeHR1cmUiLCJ0eXAiOiJKV1QifQ",
    ".",
    "eyJhdWQiOiJodHRwczovL2FwaS5ibG9ieWFyZC5sb2NhbCIsImV4cCI6MTgwMDAwMDYwMCwiaWF0IjoxODAwMDAwMDAwLCJpc3MiOiJodHRwczovL3Rva2VuLmFjdGlvbnMuZ2l0aHVidXNlcmNvbnRlbnQuY29tIiwibmJmIjoxODAwMDAwMDAwLCJyZWYiOiJyZWZzL2hlYWRzL21haW4iLCJyZXBvc2l0b3J5IjoiUmVsaWFiaWxpdHktV29ya3MvQmxvYnlhcmQtQ29yZSIsInJlcG9zaXRvcnlfb3duZXIiOiJSZWxpYWJpbGl0eS1Xb3JrcyIsInJ1bl9hdHRlbXB0IjoiMSIsInJ1bl9pZCI6IjEyMzQ1Iiwic2hhIjoiYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYSIsInN1YiI6InJlcG86UmVsaWFiaWxpdHktV29ya3NAMTIzL0Jsb2J5YXJkLUNvcmVANDU2OnJlZjpyZWZzL2hlYWRzL21haW4iLCJ3b3JrZmxvd19yZWYiOiJSZWxpYWJpbGl0eS1Xb3Jrcy9CbG9ieWFyZC1Db3JlLy5naXRodWIvd29ya2Zsb3dzL3JlbGVhc2UueW1sQHJlZnMvaGVhZHMvbWFpbiJ9",
    ".",
    "EReuMrLt-uvpjq4I8X3jgacHKQwAAEnPgvSXlTYO80UaWbrWHuEk2CX3d7ws08i7yx-r0qa6_G0vsQewrnc3tyWCCm9vfx3XpAiv2Jf92tzw50JAPffOwKdUT2gGDegj7Crdfgj9gnTjtxsGY1fGdQagORxxvkofEPvdbsZ-ynoATRIwPoXGgEyl8wPpfAhUxczVs2O1kwmeei7v0-YPkXkUwK8cw-0q2XQfLe1LIjfsmOUlyCH6YsVog8b6JD3ozak8rWt4b-40K3fNYd-A58JVb2VyjOOOiMUV9kn6yGsKlA7CDg8NH45HYrguPFPXYJh7uzvK807fyByv1t8vfg"
);

pub(super) fn signing_key() -> Jwk {
    serde_json::from_value(json!({
        "alg": "RS256",
        "e": "AQAB",
        "kid": "fixture",
        "kty": "RSA",
        "n": "0LJy5SQy15d12kuYvGzrJiB3fyeq4nOCVEkF-xN-cvzvsI4u8fUqu6v3zc1BWtSgBA4q7I9PJQ6qSiUnO5wL9Xh0XGUAY37ww1YdQIZaI71rgz6P9rqbFD5WUCyGxPmL0mLV56XFkyTzowA-O_iZDaIKZbTujgMws5aXnbAghWO7TMCV8VhK-SfNUIpqcaKwEFYGt7YmPprNXMg-FrwWC5TucjbSS1zHZaE7boiMrOET3lXZdMMqj58yMWWWWxvT45a0R1bMTkBAdFQtKlnIn8T1Va3rweAII7Pl8cnGFbxVnzD6MaBxbRCHS72CTcHAKULw5d0Z9kxyPlY0oi7OiQ",
        "use": "sig"
    }))
    .expect("signing JWK")
}

#[test]
fn claim_test_seam_accepts_valid_claims_and_rejects_malformed_json() {
    let claims = json!({
        "aud": "https://api.blobyard.local",
        "exp": 1_800_000_600_u64,
        "iat": 1_800_000_000_u64,
        "iss": GITHUB_OIDC_ISSUER,
        "nbf": 1_800_000_000_u64,
        "ref": "refs/heads/main",
        "repository": "Reliability-Works/Blobyard-Core",
        "repository_owner": "Reliability-Works",
        "run_id": "12345",
        "sub": "repo:Reliability-Works/Blobyard-Core:ref:refs/heads/main",
        "workflow_ref": "Reliability-Works/Blobyard-Core/.github/workflows/release.yml@refs/heads/main"
    });
    assert!(verify_test_claims(claims, "https://api.blobyard.local", 1_800_000_000_000).is_ok());
    assert_eq!(
        verify_test_claims(json!({}), "https://api.blobyard.local", 1_800_000_000_000),
        Err(OidcVerificationError::Invalid)
    );
}

#[tokio::test]
async fn unavailable_verifier_returns_only_the_provider_failure_class() {
    assert_eq!(
        UnavailableGithubOidcVerifier
            .verify("redacted", "https://api.example", 1)
            .await,
        Err(OidcVerificationError::ProviderUnavailable)
    );
}

#[test]
fn key_validation_requires_rs256_signing_authority() {
    let valid: Jwk = serde_json::from_value(json!({
        "alg": "RS256",
        "e": "AQAB",
        "kid": "fixture",
        "kty": "RSA",
        "n": "sXch",
        "use": "sig"
    }))
    .expect("valid JWK");
    assert_eq!(valid_key(&valid), Ok(valid.clone()));
    let without_usage = serde_json::from_value(json!({
        "alg": "RS256", "e": "AQAB", "kid": "fixture", "kty": "RSA", "n": "sXch"
    }))
    .expect("JWK without public use");
    assert_eq!(valid_key(&without_usage), Ok(without_usage.clone()));
    for value in [
        json!({ "alg": "RS384", "e": "AQAB", "kid": "fixture", "kty": "RSA", "n": "sXch", "use": "sig" }),
        json!({ "alg": "RS256", "e": "AQAB", "kid": "fixture", "kty": "RSA", "n": "sXch", "use": "enc" }),
    ] {
        let key = serde_json::from_value(value).expect("invalid JWK fixture");
        assert_eq!(valid_key(&key), Err(OidcVerificationError::Invalid));
    }
}

#[test]
fn malformed_decoding_key_is_invalid() {
    let malformed = serde_json::from_value(json!({
        "alg": "RS256", "e": "!", "kid": "fixture", "kty": "RSA", "n": "!", "use": "sig"
    }))
    .expect("malformed RSA JWK");
    assert_eq!(
        verify_with_key(
            SIGNED_TOKEN,
            &malformed,
            "https://api.blobyard.local",
            1_800_000_000_000,
        ),
        Err(OidcVerificationError::Invalid)
    );
}

#[test]
fn signed_fixture_requires_the_matching_key_and_untampered_claims() {
    let identity = verify_with_key(
        SIGNED_TOKEN,
        &signing_key(),
        "https://api.blobyard.local",
        1_800_000_000_000,
    )
    .expect("verified signed token");
    assert_eq!(identity.run_id, "12345");
    assert_eq!(identity.repository, "reliability-works/blobyard-core");

    let tampered = SIGNED_TOKEN.replacen("eyJhdWQi", "eyJhdWQj", 1);
    assert_eq!(
        verify_with_key(
            &tampered,
            &signing_key(),
            "https://api.blobyard.local",
            1_800_000_000_000,
        ),
        Err(OidcVerificationError::Invalid)
    );
}
