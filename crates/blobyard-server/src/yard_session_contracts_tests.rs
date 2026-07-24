#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{derive_key, issue, normalize_return_path, verify};
use blobyard_core::SecretString;

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
    assert_eq!(normalize_return_path("/.blobyard/session/exchange"), "/");
    assert_eq!(normalize_return_path("/docs"), "/docs");
}
