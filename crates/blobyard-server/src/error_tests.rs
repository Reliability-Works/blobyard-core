#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{ApiError, ServerError};
use axum::response::IntoResponse;
use blobyard_contract::{RepositoryError, StorageError};
use blobyard_core::ErrorCode;

#[test]
fn server_errors_have_complete_redaction_safe_messages() {
    for (error, expected) in [
        (
            ServerError::DataDirectory,
            "standalone data directory is unavailable",
        ),
        (
            ServerError::Repository(RepositoryError::Conflict),
            "metadata conflict",
        ),
        (
            ServerError::Storage,
            "standalone object storage is unavailable",
        ),
        (
            ServerError::PublicOrigin,
            "standalone public origin is invalid",
        ),
        (
            ServerError::WebYardOrigin,
            "standalone Web Yard origin is invalid",
        ),
        (
            ServerError::Initialization,
            "standalone runtime initialization failed",
        ),
    ] {
        assert_eq!(error.to_string(), expected);
    }
    assert_eq!(
        ServerError::from(RepositoryError::NotFound),
        ServerError::Repository(RepositoryError::NotFound)
    );
}

#[test]
fn repository_and_storage_failures_map_to_stable_http_classes() {
    for (error, expected) in [
        (RepositoryError::NotFound, ErrorCode::NotFound),
        (RepositoryError::Conflict, ErrorCode::Conflict),
        (RepositoryError::InvalidInput, ErrorCode::InvalidRequest),
        (RepositoryError::SchemaTooNew, ErrorCode::InternalError),
        (RepositoryError::Unavailable, ErrorCode::InternalError),
    ] {
        assert_eq!(ApiError::from_repository(error).code, expected);
    }
    for (error, expected) in [
        (StorageError::NotFound, ErrorCode::NotFound),
        (StorageError::Conflict, ErrorCode::Conflict),
        (StorageError::InvalidInput, ErrorCode::InvalidRequest),
        (StorageError::IntegrityMismatch, ErrorCode::InvalidRequest),
        (StorageError::Unavailable, ErrorCode::InternalError),
    ] {
        assert_eq!(ApiError::from_storage(error).code, expected);
    }
}

#[test]
fn range_error_response_uses_the_stable_invalid_request_envelope() {
    let response = ApiError::range_not_satisfiable().into_response();
    assert_eq!(
        response.status(),
        axum::http::StatusCode::RANGE_NOT_SATISFIABLE
    );
}
