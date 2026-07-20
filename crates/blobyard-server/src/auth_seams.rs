use axum::{http::StatusCode, response::IntoResponse};
use blobyard_contract::RepositoryError;

#[cfg(test)]
pub(crate) fn authenticate_at(
    state: &crate::api::AppState,
    raw: &str,
    now_ms: u64,
) -> Result<super::Principal, crate::error::ApiError> {
    let secret = blobyard_core::SecretString::new(raw.to_owned())
        .map_err(|_error| crate::error::ApiError::invalid_token())?;
    super::authenticate(state, &secret, Ok(now_ms))
}

/// Classifies every repository error exposed by credential lookup.
#[must_use]
pub fn credential_failure_statuses() -> [StatusCode; 5] {
    [
        RepositoryError::NotFound,
        RepositoryError::InvalidInput,
        RepositoryError::Conflict,
        RepositoryError::SchemaTooNew,
        RepositoryError::Unavailable,
    ]
    .map(|error| super::credential_error(error).into_response().status())
}
