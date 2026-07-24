#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{clear_header, read, set_header};
use axum::http::{HeaderMap, HeaderValue, header};
use blobyard_core::SecretString;

fn token() -> SecretString {
    SecretString::new(format!("byys_{}", "a".repeat(64))).expect("valid fixture")
}

#[test]
fn reads_only_the_exact_well_shaped_cookie() {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::COOKIE,
        HeaderValue::from_static(
            "other=value; __Host-blobyard-yard-session=byys_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
    );
    assert_eq!(
        read(&headers).map(|value| value.expose_secret().to_owned()),
        Some(format!("byys_{}", "a".repeat(64)))
    );

    headers.insert(
        header::COOKIE,
        HeaderValue::from_static("__Host-blobyard-yard-session=byys_NOT_HEX"),
    );
    assert!(read(&headers).is_none());

    headers.insert(
        header::COOKIE,
        HeaderValue::from_static(
            "__Host-blobyard-yard-session=byys_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; __Host-blobyard-yard-session=byys_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ),
    );
    assert!(read(&headers).is_none());
}

#[test]
fn set_and_clear_cookie_keep_the_fixed_security_attributes() {
    let set_header = set_header(&token()).expect("header");
    let set = set_header.to_str().expect("ascii");
    assert!(set.contains("Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=43200"));
    assert_eq!(
        clear_header().to_str().expect("ascii"),
        "__Host-blobyard-yard-session=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0"
    );
}
