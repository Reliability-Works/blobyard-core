use axum::{
    Json,
    http::{HeaderValue, StatusCode, header::RETRY_AFTER},
    response::IntoResponse,
};
use blobyard_contract::{RepositoryError, StorageError};
use blobyard_core::ErrorCode;
use serde::Serialize;
use std::fmt::{Display, Formatter};

/// Redaction-safe standalone server initialization failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerError {
    /// A durable data directory could not be prepared.
    DataDirectory,
    /// Metadata could not be opened or migrated.
    Repository(RepositoryError),
    /// Object storage could not be opened.
    Storage,
    /// The configured public transfer origin is not a root HTTP or HTTPS URL.
    PublicOrigin,
    /// The configured Web Yard origin cannot safely host first-level public subdomains.
    WebYardOrigin,
    /// A required local invariant could not be established.
    Initialization,
}

impl Display for ServerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::DataDirectory => "standalone data directory is unavailable",
            Self::Repository(error) => return Display::fmt(error, formatter),
            Self::Storage => "standalone object storage is unavailable",
            Self::PublicOrigin => "standalone public origin is invalid",
            Self::WebYardOrigin => "standalone Web Yard origin is invalid",
            Self::Initialization => "standalone runtime initialization failed",
        })
    }
}

impl std::error::Error for ServerError {}

impl From<RepositoryError> for ServerError {
    fn from(value: RepositoryError) -> Self {
        Self::Repository(value)
    }
}

#[derive(Debug)]
pub(crate) struct ApiError {
    code: ErrorCode,
    message: &'static str,
    retry_after_seconds: Option<u64>,
    status: StatusCode,
}

impl ApiError {
    pub(crate) const fn invalid_request() -> Self {
        Self::new(ErrorCode::InvalidRequest, StatusCode::BAD_REQUEST)
    }

    pub(crate) const fn auth_required() -> Self {
        Self::new(ErrorCode::AuthRequired, StatusCode::UNAUTHORIZED)
    }

    pub(crate) const fn invalid_token() -> Self {
        Self::new(ErrorCode::InvalidToken, StatusCode::UNAUTHORIZED)
    }

    pub(crate) const fn forbidden() -> Self {
        Self::new(ErrorCode::Forbidden, StatusCode::FORBIDDEN)
    }

    pub(crate) const fn conflict() -> Self {
        Self::new(ErrorCode::Conflict, StatusCode::CONFLICT)
    }

    pub(crate) const fn not_found() -> Self {
        Self::new(ErrorCode::NotFound, StatusCode::NOT_FOUND)
    }

    pub(crate) const fn range_not_satisfiable() -> Self {
        Self::new(ErrorCode::InvalidRequest, StatusCode::RANGE_NOT_SATISFIABLE)
    }

    pub(crate) const fn internal() -> Self {
        Self::new(ErrorCode::InternalError, StatusCode::INTERNAL_SERVER_ERROR)
    }

    pub(crate) const fn provider_unavailable() -> Self {
        Self::new(
            ErrorCode::ProviderUnavailable,
            StatusCode::SERVICE_UNAVAILABLE,
        )
    }

    pub(crate) const fn rate_limited(retry_after_seconds: u64) -> Self {
        Self {
            code: ErrorCode::RateLimited,
            message: ErrorCode::RateLimited.default_message(),
            retry_after_seconds: Some(retry_after_seconds),
            status: StatusCode::TOO_MANY_REQUESTS,
        }
    }

    pub(crate) fn invalid_request_result<T, E>(value: Result<T, E>) -> Result<T, Self> {
        value.or(Err(Self::invalid_request()))
    }

    pub(crate) fn invalid_token_result<T, E>(value: Result<T, E>) -> Result<T, Self> {
        value.or(Err(Self::invalid_token()))
    }

    pub(crate) fn not_found_result<T, E>(value: Result<T, E>) -> Result<T, Self> {
        value.or(Err(Self::not_found()))
    }

    pub(crate) fn internal_result<T, E>(value: Result<T, E>) -> Result<T, Self> {
        value.or(Err(Self::internal()))
    }

    const fn new(code: ErrorCode, status: StatusCode) -> Self {
        Self {
            code,
            message: code.default_message(),
            retry_after_seconds: None,
            status,
        }
    }

    pub(crate) const fn from_repository(error: RepositoryError) -> Self {
        match error {
            RepositoryError::NotFound => Self::not_found(),
            RepositoryError::Conflict => Self::conflict(),
            RepositoryError::InvalidInput => Self::invalid_request(),
            RepositoryError::SchemaTooNew | RepositoryError::Unavailable => Self::internal(),
        }
    }

    pub(crate) const fn concealed_capability(error: RepositoryError) -> Self {
        match error {
            RepositoryError::NotFound | RepositoryError::InvalidInput => Self::not_found(),
            RepositoryError::Conflict
            | RepositoryError::SchemaTooNew
            | RepositoryError::Unavailable => Self::internal(),
        }
    }

    pub(crate) const fn from_storage(error: StorageError) -> Self {
        match error {
            StorageError::NotFound => Self::not_found(),
            StorageError::Conflict => Self::conflict(),
            StorageError::InvalidInput | StorageError::IntegrityMismatch => Self::invalid_request(),
            StorageError::Unavailable => Self::internal(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorEnvelope {
    ok: bool,
    error: ErrorBody,
    request_id: String,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let envelope = ErrorEnvelope {
            ok: false,
            error: ErrorBody {
                code: self.code.as_str(),
                message: self.message,
            },
            request_id: request_id(),
        };
        let mut response = (self.status, Json(envelope)).into_response();
        if let Some(seconds) = self.retry_after_seconds
            && let Ok(value) = HeaderValue::from_str(&seconds.to_string())
        {
            response.headers_mut().insert(RETRY_AFTER, value);
        }
        response
    }
}

pub(crate) fn request_id() -> String {
    format!("req_{}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
