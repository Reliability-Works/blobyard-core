#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    CONTINUATION_PREFIX, ContinuationClaims, HmacSha256, canonical_payload, decoded_signature,
    derive_key, encoded_continuation, has_token_shape, identity_authority, issue, issue_payload,
    issued_secret, login_url, normalize_return_path, signature, signature_from, signer_result,
    valid_host_label, validate_claims, verified_payload, verifier_result, verify, yard_host_label,
    yard_url,
};
use crate::test_support::error_status;
use axum::http::StatusCode;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use blobyard_core::SecretString;
use hmac::Mac;

fn key() -> ([u8; 32], SecretString) {
    let capability = SecretString::new("test-capability").expect("valid fixture");
    (derive_key(&capability), capability)
}

#[test]
fn continuation_round_trips_exact_host_and_normalized_path() {
    let (key, _capability) = key();
    let continuation = issue(&key, "docs-a1b2c3d4e-workspace", "/guide?q=1", 100).expect("issued");
    let claims = verify(&key, &continuation, 101).expect("verified");
    assert_eq!(claims.host_label(), "docs-a1b2c3d4e-workspace");
    assert_eq!(claims.return_path(), "/guide?q=1");
}

#[test]
fn continuation_rejects_expiry_and_tampering() {
    let (key, _capability) = key();
    let continuation = issue(&key, "docs-a1b2c3d4e-workspace", "/", 100).expect("issued");
    assert!(verify(&key, &continuation, 600_100).is_err());
    let tampered =
        SecretString::new(format!("{}0", continuation.expose_secret())).expect("valid secret");
    assert!(verify(&key, &tampered, 101).is_err());
}

#[test]
fn return_path_falls_back_for_external_and_reserved_paths() {
    assert_eq!(normalize_return_path("//elsewhere.example"), "/");
    assert_eq!(normalize_return_path("/\\elsewhere.example"), "/");
    assert_eq!(normalize_return_path("/.blobyard"), "/");
    assert_eq!(normalize_return_path("/.blobyard/session/exchange"), "/");
    assert_eq!(normalize_return_path("/bad\npath"), "/");
    assert_eq!(
        normalize_return_path(&format!("/{}", "x".repeat(2_049))),
        "/"
    );
    assert_eq!(normalize_return_path("/docs"), "/docs");
}

fn signed(key: &[u8; 32], payload: &[u8]) -> SecretString {
    SecretString::new(format!(
        "{CONTINUATION_PREFIX}{}.{}",
        URL_SAFE_NO_PAD.encode(payload),
        hex::encode(signature(key, payload).expect("signature"))
    ))
    .expect("continuation")
}

fn claims() -> ContinuationClaims {
    ContinuationClaims {
        e: 1_000,
        h: "docs-a1b2c3d4e-workspace".to_owned(),
        n: "a".repeat(32),
        p: "/docs".to_owned(),
        v: 1,
    }
}

#[test]
fn issuance_rejects_invalid_boundaries() {
    let (key, _capability) = key();
    assert_eq!(
        error_status(issue(&key, "invalid", "/", 1)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(issue(&key, "docs-a1b2c3d4e-workspace", "/", u64::MAX,)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn public_url_helpers_reject_invalid_boundaries() {
    assert_eq!(
        identity_authority("https://example.test:8443/path").as_deref(),
        Some("example.test:8443")
    );
    assert_eq!(identity_authority("not a URL"), None);
    assert_eq!(identity_authority("file:///tmp/blobyard"), None);
    assert_eq!(
        yard_host_label(
            "https://blobyard.test",
            "docs-a1b2c3d4e-workspace.blobyard.test"
        )
        .as_deref(),
        Some("docs-a1b2c3d4e-workspace")
    );
    assert_eq!(yard_host_label("not a URL", "docs.blobyard.test"), None);
    assert_eq!(
        yard_host_label("https://blobyard.test", "elsewhere.test"),
        None
    );
    assert_eq!(
        yard_host_label("https://blobyard.test", "invalid.blobyard.test"),
        None
    );
    assert_eq!(
        error_status(yard_url("not a URL", "docs-a1b2c3d4e-workspace")),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(yard_url("https://blobyard.test", "bad label")),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let (key, _capability) = key();
    let continuation = issue(&key, "docs-a1b2c3d4e-workspace", "/", 1).expect("issued");
    assert_eq!(
        error_status(login_url("not a URL", &continuation)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn verification_rejects_every_malformed_or_inconsistent_claim() {
    let (key, _capability) = key();
    for malformed in [
        "not-a-continuation".to_owned(),
        "byc_no-separator".to_owned(),
        format!("byc_.{}", "a".repeat(64)),
        "byc_e30.short".to_owned(),
        format!("byc_e30.{}", "A".repeat(64)),
        format!("byc_!.{}", "a".repeat(64)),
    ] {
        let malformed = SecretString::new(malformed).expect("malformed fixture");
        assert!(verify(&key, &malformed, 1).is_err());
    }
    assert!(verify(&key, &signed(&key, b"not json"), 1).is_err());

    let noncanonical = br#"{ "e":1000,"h":"docs-a1b2c3d4e-workspace","n":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","p":"/docs","v":1}"#;
    assert!(verify(&key, &signed(&key, noncanonical), 1).is_err());
    for inconsistent in [
        ContinuationClaims { v: 2, ..claims() },
        ContinuationClaims { e: 1, ..claims() },
        ContinuationClaims {
            h: "invalid".to_owned(),
            ..claims()
        },
        ContinuationClaims {
            n: "A".repeat(32),
            ..claims()
        },
    ] {
        let payload = serde_json::to_vec(&inconsistent).expect("claims");
        assert!(verify(&key, &signed(&key, &payload), 1).is_err());
    }
    let other_key = [7_u8; 32];
    let payload = serde_json::to_vec(&claims()).expect("claims");
    assert!(verify(&other_key, &signed(&key, &payload), 1).is_err());
}

#[test]
fn token_and_host_shape_helpers_cover_positive_and_negative_inputs() {
    assert!(has_token_shape(&format!("byx_{}", "a".repeat(64)), "byx_"));
    assert!(!has_token_shape("wrong", "byx_"));
    assert!(!has_token_shape(&format!("byx_{}", "A".repeat(64)), "byx_"));
    assert!(valid_host_label("docs-a1b2c3d4e-workspace"));
    assert!(!valid_host_label("docs"));
}

#[test]
fn issuance_encoders_map_injected_failures_safely() {
    let json_error = serde_json::from_str::<Vec<u8>>("{").expect_err("malformed JSON fixture");
    assert_eq!(
        error_status(issue_payload(Err(json_error))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(issued_secret(SecretString::new(""))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(signer_result(Err(hmac::digest::InvalidLength))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let json_error = serde_json::from_str::<Vec<u8>>("{").expect_err("malformed JSON fixture");
    assert_eq!(
        error_status(encoded_continuation(
            Err(json_error),
            Err(hmac::digest::InvalidLength),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let payload = serde_json::to_vec(&claims()).expect("claims");
    assert_eq!(
        error_status(encoded_continuation(
            Ok(payload.clone()),
            Err(hmac::digest::InvalidLength),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(signature_from(Err(hmac::digest::InvalidLength), &payload,)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn verification_decoders_map_injected_failures_safely() {
    assert!(decoded_signature(hex::decode("x")).is_err());
    let json_error = serde_json::from_str::<Vec<u8>>("{").expect_err("malformed JSON fixture");
    assert!(canonical_payload(Err(json_error)).is_err());
    assert!(verifier_result(Err(hmac::digest::InvalidLength)).is_err());
    let payload = serde_json::to_vec(&claims()).expect("claims");
    assert!(
        verified_payload(
            &payload,
            hex::decode("x"),
            HmacSha256::new_from_slice(&key().0),
        )
        .is_err()
    );
    assert!(
        verified_payload(&payload, Ok(vec![0; 32]), Err(hmac::digest::InvalidLength),).is_err()
    );
    let json_error = serde_json::from_str::<Vec<u8>>("{").expect_err("malformed JSON fixture");
    assert!(validate_claims(claims(), &payload, 0, Err(json_error)).is_err());
}
