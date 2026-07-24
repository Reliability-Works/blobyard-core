use super::{exchange_failure, logout_result};
use crate::test_support::error_status;
use axum::http::StatusCode;
use blobyard_contract::RepositoryError;

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
