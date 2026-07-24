#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    exchange_at, exchange_failure, exchanged_redirect, expected_origin, fresh_login_redirect_at,
    login_redirect_at, logout_result, parsed_origin, require_same_origin, revoke_cookie,
    session_cookie_result, single_code,
};
use crate::test_support::error_status;
use axum::http::{HeaderMap, StatusCode};
use blobyard_contract::RepositoryError;
use blobyard_core::SecretString;

fn state() -> (tempfile::TempDir, crate::api::AppState) {
    let root = tempfile::tempdir().expect("root");
    let state = crate::test_support::filesystem_state(&root, root.path().join("staging"));
    (root, state)
}

#[test]
fn exchange_redirects_only_expected_authentication_misses() {
    assert!(exchange_failure(RepositoryError::NotFound).is_ok());
    for error in [
        RepositoryError::Conflict,
        RepositoryError::InvalidInput,
        RepositoryError::SchemaTooNew,
        RepositoryError::Unavailable,
    ] {
        assert_eq!(
            error_status(exchange_failure(error)),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[test]
fn logout_is_idempotent_but_surfaces_repository_failures() {
    assert!(logout_result(Ok(true)).is_ok());
    assert!(logout_result(Ok(false)).is_ok());
    for error in [
        RepositoryError::NotFound,
        RepositoryError::Conflict,
        RepositoryError::InvalidInput,
        RepositoryError::SchemaTooNew,
        RepositoryError::Unavailable,
    ] {
        assert_eq!(
            error_status(logout_result(Err(error))),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[test]
fn exchange_query_accepts_exactly_one_code() {
    assert_eq!(single_code("code=one").as_deref(), Some("one"));
    assert_eq!(single_code("wrong=one"), None);
    assert_eq!(single_code("code=one&code=two"), None);
}

#[test]
fn redirects_reject_invalid_header_values() {
    assert!(crate::response::redirect(StatusCode::FOUND, "bad\nlocation", None).is_err());
}

#[test]
fn runtime_clock_and_overflow_failures_are_internal() {
    let (_root, state) = state();
    let code = SecretString::new(format!("byx_{}", "a".repeat(64))).expect("code");
    let failure = || Err(crate::error::ApiError::internal());

    for result in [
        exchange_at(&state, "docs-fixture", &code, failure()),
        exchange_at(&state, "docs-fixture", &code, Ok(u64::MAX)),
        fresh_login_redirect_at(&state, "docs-fixture", failure()),
        fresh_login_redirect_at(&state, "docs-fixture", Ok(u64::MAX)),
        login_redirect_at(&state, "docs-fixture", "/", failure()),
        login_redirect_at(&state, "docs-fixture", "/", Ok(u64::MAX)),
    ] {
        assert_eq!(error_status(result), StatusCode::INTERNAL_SERVER_ERROR);
    }
    assert_eq!(
        error_status(revoke_cookie(&state, "docs-fixture", &code, failure(),)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn cookie_and_origin_helpers_map_failures() {
    assert_eq!(
        error_status(session_cookie_result(Err(()))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(exchanged_redirect(
            "/",
            Err(crate::error::ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(parsed_origin("not a URL")),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let (_root, state) = state();
    assert!(require_same_origin(&state, "docs-fixture", &HeaderMap::new()).is_ok());
    assert!(parsed_origin("http://localhost:8787").is_ok());
    assert_eq!(
        error_status(expected_origin(Ok("not a URL".to_owned()))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(expected_origin(Err(crate::error::ApiError::internal()))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(session_cookie_result(Ok(axum::http::HeaderValue::from_static("cookie"))).is_ok());
}

#[test]
fn configured_origins_fail_closed() {
    let (_root, state) = state();
    let mut invalid_public_origin = state.clone();
    invalid_public_origin.public_origin = "not a URL".to_owned();
    assert_eq!(
        error_status(fresh_login_redirect_at(
            &invalid_public_origin,
            "docs-fixture",
            Ok(1),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(login_redirect_at(
            &invalid_public_origin,
            "docs-fixture",
            "/",
            Ok(1),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let mut invalid_yard_origin = state;
    invalid_yard_origin.web_yard_origin = "not a URL".to_owned();
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::ORIGIN,
        axum::http::HeaderValue::from_static("http://docs-fixture.localhost:8787"),
    );
    assert_eq!(
        error_status(require_same_origin(
            &invalid_yard_origin,
            "docs-fixture",
            &headers,
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
