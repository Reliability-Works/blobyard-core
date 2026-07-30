#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    access_token, callback_uri, secure_endpoint, test_support::*, validate_authorized_party,
    validate_not_before_payload, verified_email,
};
use crate::ServerError;
use crate::yard_oidc_provider::YardOidcProviderError;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use blobyard_testkit::FixtureExecutionTracker;
use openidconnect::{
    AccessToken, EndUserEmail, Nonce, SubjectIdentifier,
    core::{CoreIdToken, CoreJwsSigningAlgorithm, CoreTokenResponse, CoreUserInfoClaims},
};

fn assert_authorization(tracker: &mut FixtureExecutionTracker) {
    assert_authorization_contract();
    tracker.record_case(
        "oidc-authorization-uses-fixed-scopes-pkce-and-nonce",
        &serde_json::json!({"flow": "authorization-code"}),
        &serde_json::json!({
            "scopes": ["openid", "email", "profile"],
            "pkceMethod": "S256",
            "nonceRequired": true
        }),
    );
}

fn assert_issuer_and_audience(tracker: &mut FixtureExecutionTracker) {
    let valid = token(&base_claims(), CLIENT_SECRET);
    let valid_verifier = verifier(CLIENT_SECRET);
    assert!(
        valid
            .claims(&valid_verifier, &Nonce::new(NONCE.to_owned()))
            .is_ok()
    );
    let mut wrong_issuer = base_claims();
    wrong_issuer["iss"] = serde_json::json!("https://other.example.test/");
    assert!(
        token(&wrong_issuer, CLIENT_SECRET)
            .claims(&valid_verifier, &Nonce::new(NONCE.to_owned()))
            .is_err()
    );
    tracker.record_case(
        "oidc-issuer-mismatch-is-rejected",
        &serde_json::json!({"issuerMatchesDiscovery": false}),
        &serde_json::json!({"admitted": false}),
    );

    let mut wrong_audience = base_claims();
    wrong_audience["aud"] = serde_json::json!(["another-client"]);
    assert!(
        token(&wrong_audience, CLIENT_SECRET)
            .claims(&valid_verifier, &Nonce::new(NONCE.to_owned()))
            .is_err()
    );
    tracker.record_case(
        "oidc-audience-mismatch-is-rejected",
        &serde_json::json!({"audienceContainsClient": false}),
        &serde_json::json!({"admitted": false}),
    );
}

fn assert_authorized_party_and_time(tracker: &mut FixtureExecutionTracker) {
    assert!(validate_authorized_party(&claims(&[CLIENT_ID], None), CLIENT_ID).is_ok());
    assert!(
        validate_authorized_party(&claims(&[CLIENT_ID, "other"], Some(CLIENT_ID)), CLIENT_ID,)
            .is_ok()
    );
    assert!(
        validate_authorized_party(&claims(&[CLIENT_ID, "other"], Some("other")), CLIENT_ID)
            .is_err()
    );
    tracker.record_case(
        "oidc-authorized-party-is-exact-for-multiple-audiences",
        &serde_json::json!({"audienceCount": 2, "authorizedPartyMatchesClient": false}),
        &serde_json::json!({"admitted": false}),
    );

    let valid_verifier = verifier(CLIENT_SECRET);
    let mut expired = base_claims();
    expired["exp"] = serde_json::json!(1);
    assert!(
        token(&expired, CLIENT_SECRET)
            .claims(&valid_verifier, &Nonce::new(NONCE.to_owned()))
            .is_err()
    );
    assert_eq!(
        validate_not_before_payload(
            &format!(
                "header.{}.signature",
                URL_SAFE_NO_PAD.encode(br#"{"nbf":101}"#)
            ),
            100_999,
        ),
        Err(YardOidcProviderError::InvalidResponse)
    );
    let invalid_json = URL_SAFE_NO_PAD.encode(b"not-json");
    for malformed in [
        "header.signature".to_owned(),
        format!("header.{invalid_json}.signature"),
    ] {
        assert_eq!(
            validate_not_before_payload(&malformed, 100_999),
            Err(YardOidcProviderError::InvalidResponse)
        );
    }
    tracker.record_case(
        "oidc-expiry-and-not-before-are-enforced",
        &serde_json::json!({"callbackWithinTimeWindow": false}),
        &serde_json::json!({"admitted": false}),
    );
}

fn assert_nonce_and_subject(tracker: &mut FixtureExecutionTracker) {
    let valid = token(&base_claims(), CLIENT_SECRET);
    let valid_verifier = verifier(CLIENT_SECRET);
    assert!(
        valid
            .claims(&valid_verifier, &Nonce::new("wrong-nonce".to_owned()))
            .is_err()
    );
    tracker.record_case(
        "oidc-nonce-mismatch-is-rejected",
        &serde_json::json!({"nonceMatchesAttempt": false}),
        &serde_json::json!({"admitted": false}),
    );

    assert!(
        serde_json::from_value::<openidconnect::core::CoreIdTokenClaims>(serde_json::json!({
            "iss": ISSUER, "aud": [CLIENT_ID], "exp": 200, "iat": 100
        }))
        .is_err()
    );
    for invalid in ["", "provider\nsubject", &"s".repeat(513)] {
        assert!(!blobyard_contract::is_valid_oidc_provider_subject(invalid));
    }
    tracker.record_case(
        "oidc-subject-is-required-and-bounded",
        &serde_json::json!({"subjectShape": "missing-empty-control-or-oversized"}),
        &serde_json::json!({"admitted": false}),
    );
}

fn assert_user_info(tracker: &mut FixtureExecutionTracker) {
    let expected_subject = SubjectIdentifier::new("provider-subject".to_owned());
    assert!(
        CoreUserInfoClaims::from_json::<reqwest::Error>(
            br#"{"sub":"other-subject","email":"person@example.test","email_verified":true}"#,
            Some(&expected_subject),
        )
        .is_err()
    );
    let unverified_email = EndUserEmail::new("person@example.test".to_owned());
    assert_eq!(verified_email(Some(&unverified_email), Some(false)), None);
    tracker.record_case(
        "oidc-userinfo-subject-and-verified-email-are-required",
        &serde_json::json!({"claimsVerifiedEmail": null, "userInfoSubjectMatches": false}),
        &serde_json::json!({"admitted": false}),
    );
}

fn assert_unsupported_access_token_hashes(response: &CoreTokenResponse) {
    let mut unsigned_claims = base_claims();
    unsigned_claims["at_hash"] = serde_json::json!("wrong-hash");
    let unsigned_claims = serde_json::from_value(unsigned_claims).expect("unsigned claims");
    let unsigned_header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
    let unsigned_payload =
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&unsigned_claims).expect("unsigned payload"));
    let unsigned: openidconnect::core::CoreIdToken = serde_json::from_value(
        serde_json::Value::String(format!("{unsigned_header}.{unsigned_payload}.")),
    )
    .expect("unsigned ID token");
    assert_eq!(
        access_token::validate(&unsigned, &unsigned_claims, response),
        Err(YardOidcProviderError::InvalidResponse)
    );
    let eddsa_header = URL_SAFE_NO_PAD.encode(br#"{"alg":"EdDSA"}"#);
    let eddsa_signature = URL_SAFE_NO_PAD.encode(b"signature");
    let eddsa: openidconnect::core::CoreIdToken =
        serde_json::from_value(serde_json::Value::String(format!(
            "{eddsa_header}.{unsigned_payload}.{eddsa_signature}"
        )))
        .expect("unsupported signed ID token");
    assert_eq!(
        access_token::validate(&eddsa, &unsigned_claims, response),
        Err(YardOidcProviderError::InvalidResponse)
    );

    let access_token = AccessToken::new("provider-access-value".to_owned());
    for algorithm in [
        CoreJwsSigningAlgorithm::HmacSha256,
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
        CoreJwsSigningAlgorithm::RsaSsaPssSha256,
        CoreJwsSigningAlgorithm::EcdsaP256Sha256,
        CoreJwsSigningAlgorithm::HmacSha384,
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha384,
        CoreJwsSigningAlgorithm::RsaSsaPssSha384,
        CoreJwsSigningAlgorithm::EcdsaP384Sha384,
        CoreJwsSigningAlgorithm::HmacSha512,
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha512,
        CoreJwsSigningAlgorithm::RsaSsaPssSha512,
        CoreJwsSigningAlgorithm::EcdsaP521Sha512,
    ] {
        assert!(access_token::hash(&algorithm, &access_token).is_ok());
    }
    assert_eq!(
        access_token::hash(&CoreJwsSigningAlgorithm::EdDsa, &access_token),
        Err(YardOidcProviderError::InvalidResponse)
    );
}

fn assert_token_integrity(tracker: &mut FixtureExecutionTracker) {
    let valid = token(&base_claims(), CLIENT_SECRET);
    let valid_verifier = verifier(CLIENT_SECRET);
    let mut claims_without_issued_at = base_claims();
    claims_without_issued_at
        .as_object_mut()
        .expect("claims object")
        .remove("iat");
    let missing_issued_at = raw_token(&claims_without_issued_at, CLIENT_SECRET);
    assert!(
        serde_json::from_value::<CoreIdToken>(serde_json::Value::String(missing_issued_at))
            .is_err(),
        "an ID token without iat is rejected"
    );
    assert!(
        valid
            .claims(&verifier("wrong-secret"), &Nonce::new(NONCE.to_owned()))
            .is_err()
    );
    let valid_claims = valid
        .claims(&valid_verifier, &Nonce::new(NONCE.to_owned()))
        .expect("claims without access token hash");
    let valid_response: CoreTokenResponse = serde_json::from_value(serde_json::json!({
        "access_token": "provider-access-value",
        "token_type": "Bearer",
        "id_token": valid.to_string()
    }))
    .expect("token response without access token hash");
    assert!(access_token::validate(&valid, valid_claims, &valid_response).is_ok());

    let mut wrong_hash = base_claims();
    wrong_hash["at_hash"] = serde_json::json!("wrong-hash");
    let wrong_hash = token(&wrong_hash, CLIENT_SECRET);
    let wrong_hash_claims = wrong_hash
        .claims(&valid_verifier, &Nonce::new(NONCE.to_owned()))
        .expect("claims before access hash validation");
    let response: CoreTokenResponse = serde_json::from_value(serde_json::json!({
        "access_token": "provider-access-value",
        "token_type": "Bearer",
        "id_token": wrong_hash.to_string()
    }))
    .expect("token response");
    assert!(access_token::validate(&wrong_hash, wrong_hash_claims, &response).is_err());
    assert_unsupported_access_token_hashes(&response);
    tracker.record_case(
        "oidc-signature-and-access-token-hash-are-enforced",
        &serde_json::json!({"signatureValid": false, "atHashMatches": false}),
        &serde_json::json!({"admitted": false}),
    );
}

fn assert_endpoint_contract() {
    assert_eq!(
        callback_uri("https://core.example.test")
            .expect("callback")
            .as_str(),
        "https://core.example.test/account/yard-oidc/callback"
    );
    for loopback in ["http://localhost:8787", "http://127.0.0.1:8787"] {
        assert_eq!(
            callback_uri(loopback).expect("loopback callback").as_str(),
            format!("{loopback}/account/yard-oidc/callback")
        );
    }
    assert_eq!(
        callback_uri("http://core.example.test").err(),
        Some(ServerError::PublicOrigin)
    );
    assert!(secure_endpoint(
        &url::Url::parse("http://localhost:9000/token").expect("loopback endpoint")
    ));
    assert!(secure_endpoint(
        &url::Url::parse("http://[::1]:9000/token").expect("IPv6 loopback endpoint")
    ));
    assert!(!secure_endpoint(
        &url::Url::parse("http://identity.example.test/token").expect("insecure endpoint")
    ));
}

#[test]
fn provider_validation_executes_every_generated_oidc_case() {
    let mut tracker = FixtureExecutionTracker::new_oidc("server", "oidc-provider-validation");
    assert_authorization(&mut tracker);
    assert_issuer_and_audience(&mut tracker);
    assert_authorized_party_and_time(&mut tracker);
    assert_nonce_and_subject(&mut tracker);
    assert_user_info(&mut tracker);
    assert_token_integrity(&mut tracker);
    assert_endpoint_contract();
    tracker.finish().expect("provider OIDC fixture coverage");
}
