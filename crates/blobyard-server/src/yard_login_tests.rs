#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::test_support::error_status;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use blobyard_contract::RepositoryError;
use blobyard_core::SecretString;
use blobyard_testkit::FixtureExecutionTracker;
use http_body_util::BodyExt;

#[test]
fn durable_continuation_ids_use_the_normative_prefix() {
    let id = super::continuation_id();
    assert!(id.starts_with("yardcont_"));
    assert_eq!(id.len(), "yardcont_".len() + 32);
}

#[test]
fn login_parameters_reject_missing_duplicate_and_unknown_fields() {
    assert_eq!(
        super::single_parameter("continuation=one", "continuation").as_deref(),
        Some("one")
    );
    assert_eq!(super::single_parameter("wrong=one", "continuation"), None);
    assert_eq!(
        super::single_parameter("continuation=one&continuation=two", "continuation"),
        None
    );

    assert_eq!(
        super::login_form("continuation=one&login_key=two"),
        Some(("one".to_owned(), "two".to_owned()))
    );
    for malformed in [
        "",
        "continuation=one",
        "login_key=two",
        "unknown=value&continuation=one&login_key=two",
        "continuation=one&continuation=two&login_key=three",
        "continuation=one&login_key=two&login_key=three",
    ] {
        assert_eq!(super::login_form(malformed), None);
    }
}

#[test]
fn redirect_rejects_invalid_header_values() {
    assert!(crate::response::redirect(StatusCode::FOUND, "bad\nlocation", None).is_err());
    assert!(super::parse_location("not a URL").is_err());
    let code = SecretString::new("code").expect("code");
    assert!(super::exchange_redirect_from_url(Ok("not a URL".to_owned()), &code).is_err());
    assert!(
        super::exchange_redirect_from_url(Err(crate::error::ApiError::internal()), &code,).is_err()
    );
}

#[test]
fn exchange_code_persistence_maps_each_stable_failure_class() {
    let code = SecretString::new(format!("byx_{}", "a".repeat(64))).expect("code");
    assert_eq!(
        super::issue_redirect(Ok(()), "http://localhost:8787", "docs-fixture", &code,)
            .expect("exchange redirect")
            .status(),
        StatusCode::SEE_OTHER
    );
    for error in [RepositoryError::Conflict, RepositoryError::NotFound] {
        assert_eq!(
            super::issue_redirect(Err(error), "http://localhost:8787", "docs-fixture", &code,)
                .expect("safe failure page")
                .status(),
            StatusCode::OK
        );
    }
    assert!(
        super::issue_redirect(
            Err(RepositoryError::Unavailable),
            "http://localhost:8787",
            "docs-fixture",
            &code,
        )
        .is_err()
    );
}

#[tokio::test]
async fn login_clock_and_expiry_failures_are_internal() {
    let root = tempfile::tempdir().expect("root");
    let state = crate::test_support::filesystem_state(&root, root.path().join("staging"));
    assert_eq!(
        error_status(super::get_at(
            &state,
            Some("continuation=one"),
            Err(crate::error::ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(
            super::post_at(
                &state,
                "fingerprint",
                Request::new(Body::empty()),
                Err(crate::error::ApiError::internal()),
            )
            .await
        ),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(super::exchange_expiry(u64::MAX)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(super::exchange_expiry(1).is_ok());
    let code = SecretString::new("code").expect("code");
    assert_eq!(
        error_status(super::issue_durable_redirect(
            &state,
            "docs-fixture",
            &code,
            Err(crate::error::ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn disabled_oidc_preserves_the_local_key_fallback() {
    let response = super::page::login("docs-fixture", "continuation", false, false);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("login page body")
        .to_bytes();
    let body = std::str::from_utf8(&body).expect("login page text");
    assert!(body.contains("action=\"/account/yard-login\""));
    assert!(body.contains("name=\"login_key\""));
    assert!(!body.contains("action=\"/account/yard-oidc/start\""));
    let enabled = super::page::login("docs-fixture", "continuation<&", false, true)
        .into_body()
        .collect()
        .await
        .expect("OIDC login page")
        .to_bytes();
    let enabled = std::str::from_utf8(&enabled).expect("OIDC login page text");
    assert!(enabled.contains("action=\"/account/yard-login\""));
    assert!(enabled.contains("action=\"/account/yard-oidc/start\""));
    assert!(enabled.contains("continuation&lt;&amp;"));

    let mut tracker = FixtureExecutionTracker::new_oidc("server", "oidc-local-key-fallback");
    tracker.record_case(
        "oidc-disabled-preserves-local-key-fallback",
        &serde_json::json!({"oidcConfigured": false}),
        &serde_json::json!({"localKeyFormPresent": true, "oidcFormPresent": false}),
    );
    tracker.finish().expect("local-key OIDC fixture coverage");
}
