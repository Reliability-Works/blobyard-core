//! Shared test fixture contract tests.

#![allow(clippy::panic, reason = "fixture setup failures must fail the test")]

use blobyard_testkit::{SAMPLE_BLOBYARD_URI, sample_blobyard_uri};

#[test]
fn sample_uri_is_valid_and_stable() {
    let result = sample_blobyard_uri();
    let Ok(uri) = result else {
        panic!("shared sample URI must remain valid");
    };

    assert_eq!(uri.to_string(), SAMPLE_BLOBYARD_URI);
}
