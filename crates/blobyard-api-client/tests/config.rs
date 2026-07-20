//! API client configuration contract tests.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test setup failures must fail the test"
)]

use blobyard_api_client::{ApiClientConfig, DEFAULT_API_BASE_URL};
use blobyard_core::ErrorCode;
use std::time::Duration;

#[test]
fn production_configuration_is_the_default() {
    let config = ApiClientConfig::new(DEFAULT_API_BASE_URL);

    assert_eq!(
        config.map(|value| value.api_base_url().to_owned()),
        Ok(DEFAULT_API_BASE_URL.to_owned())
    );
}

#[test]
fn accepts_secure_and_loopback_endpoints() {
    let cases = [
        ("https://api.example.dev/v1/", "https://api.example.dev/v1"),
        ("http://localhost:3210/v1", "http://localhost:3210/v1"),
        ("http://127.0.0.1:3210/v1", "http://127.0.0.1:3210/v1"),
        ("http://[::1]:3210/v1", "http://[::1]:3210/v1"),
    ];

    for (input, expected) in cases {
        let result = ApiClientConfig::new(input);
        let Ok(config) = result else {
            panic!("valid API endpoint should pass: {input}");
        };
        assert_eq!(config.api_base_url(), expected);
    }
}

#[test]
fn rejects_unsafe_or_ambiguous_endpoints() {
    let cases = [
        "not-a-url",
        "ftp://api.example.dev/v1",
        "http://api.example.dev/v1",
        "https://user@example.dev/v1",
        "https://user:pass@example.dev/v1",
        "https://example.dev/v1?token=secret",
        "https://example.dev/v1#fragment",
        "https://example.dev/v2",
        "https://",
    ];

    for input in cases {
        let result = ApiClientConfig::new(input);
        let Err(error) = result else {
            panic!("unsafe API endpoint should fail: {input}");
        };
        assert_eq!(error.code(), ErrorCode::InvalidRequest);
    }
}

#[test]
fn normalizes_root_paths_and_validates_timeout_bounds() {
    let config = ApiClientConfig::new("https://api.example.dev")
        .and_then(|value| value.with_timeouts(Duration::from_secs(9), Duration::from_secs(3)));
    let Ok(config) = config else {
        panic!("valid config");
    };
    assert_eq!(config.api_base_url(), "https://api.example.dev/v1");
    assert_eq!(config.request_timeout(), Duration::from_secs(9));
    assert_eq!(config.connect_timeout(), Duration::from_secs(3));
    assert!(format!("{config:?}").contains("api.example.dev"));

    let defaults = ApiClientConfig::new(DEFAULT_API_BASE_URL).expect("default config");
    assert_eq!(defaults.request_timeout(), Duration::from_secs(30));
    assert_eq!(defaults.connect_timeout(), Duration::from_secs(10));
    for (request, connect) in [
        (Duration::ZERO, Duration::from_secs(1)),
        (Duration::from_secs(1), Duration::ZERO),
        (Duration::from_secs(1), Duration::from_secs(2)),
    ] {
        assert!(
            ApiClientConfig::new(DEFAULT_API_BASE_URL)
                .and_then(|value| value.with_timeouts(request, connect))
                .is_err()
        );
    }
}
