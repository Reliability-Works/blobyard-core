//! Stable error code and display contract tests.

use blobyard_core::{BlobyardError, ErrorCode};

#[test]
fn error_codes_have_stable_strings_and_exit_classes() {
    let cases = [
        (ErrorCode::InvalidRequest, "INVALID_REQUEST", 2),
        (ErrorCode::AuthRequired, "AUTH_REQUIRED", 10),
        (ErrorCode::InvalidToken, "INVALID_TOKEN", 10),
        (ErrorCode::TokenExpired, "TOKEN_EXPIRED", 10),
        (ErrorCode::Forbidden, "FORBIDDEN", 11),
        (ErrorCode::NotFound, "NOT_FOUND", 12),
        (ErrorCode::Conflict, "CONFLICT", 13),
        (ErrorCode::PlanLimit, "PLAN_LIMIT", 14),
        (ErrorCode::OperationUnsupported, "OPERATION_UNSUPPORTED", 15),
        (ErrorCode::NetworkError, "NETWORK_ERROR", 20),
        (ErrorCode::ProviderUnavailable, "PROVIDER_UNAVAILABLE", 21),
        (ErrorCode::UploadIncomplete, "UPLOAD_INCOMPLETE", 22),
        (ErrorCode::ChecksumMismatch, "CHECKSUM_MISMATCH", 22),
        (ErrorCode::StorageError, "STORAGE_ERROR", 22),
        (ErrorCode::RateLimited, "RATE_LIMITED", 23),
        (ErrorCode::InternalError, "INTERNAL_ERROR", 70),
        (ErrorCode::Interrupted, "INTERRUPTED", 130),
    ];

    for (code, serialized, exit) in cases {
        assert_eq!(code.as_str(), serialized);
        assert_eq!(code.to_string(), serialized);
        assert_eq!(code.exit_code(), exit);
    }
}

#[test]
fn blobyard_error_exposes_safe_context() {
    let error = BlobyardError::new(ErrorCode::AuthRequired, "Sign in with blobyard login.")
        .with_request_id("req_example");

    assert_eq!(error.code(), ErrorCode::AuthRequired);
    assert_eq!(error.message(), "Sign in with blobyard login.");
    assert_eq!(error.request_id(), Some("req_example"));
    assert_eq!(
        error.to_string(),
        "[AUTH_REQUIRED] Sign in with blobyard login."
    );
}

#[test]
fn blobyard_error_omits_absent_request_context() {
    let error = BlobyardError::new(ErrorCode::InvalidRequest, "Invalid input.");

    assert_eq!(error.request_id(), None);
    assert_eq!(error.to_string(), "[INVALID_REQUEST] Invalid input.");
}
