use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

/// Stable error codes shared by Blobyard's API and command-line client.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// Request input failed validation.
    InvalidRequest,
    /// The caller must authenticate.
    AuthRequired,
    /// The supplied credential is invalid.
    InvalidToken,
    /// The supplied credential has expired.
    TokenExpired,
    /// The caller lacks permission.
    Forbidden,
    /// The requested resource does not exist.
    NotFound,
    /// The request conflicts with current state.
    Conflict,
    /// The active plan or quota blocks the request.
    PlanLimit,
    /// The selected Blob Yard deployment does not provide this operation.
    OperationUnsupported,
    /// The upload has not supplied all required bytes or parts.
    UploadIncomplete,
    /// Transferred bytes do not match the expected checksum.
    ChecksumMismatch,
    /// The caller exceeded a rate limit.
    RateLimited,
    /// An external provider is unavailable.
    ProviderUnavailable,
    /// A network, DNS, or TLS operation failed.
    NetworkError,
    /// A storage transfer or integrity operation failed.
    StorageError,
    /// An unexpected internal invariant failed.
    InternalError,
    /// The user interrupted the operation.
    Interrupted,
}

impl ErrorCode {
    /// Returns the stable serialized code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::AuthRequired => "AUTH_REQUIRED",
            Self::InvalidToken => "INVALID_TOKEN",
            Self::TokenExpired => "TOKEN_EXPIRED",
            Self::Forbidden => "FORBIDDEN",
            Self::NotFound => "NOT_FOUND",
            Self::Conflict => "CONFLICT",
            Self::PlanLimit => "PLAN_LIMIT",
            Self::OperationUnsupported => "OPERATION_UNSUPPORTED",
            Self::UploadIncomplete => "UPLOAD_INCOMPLETE",
            Self::ChecksumMismatch => "CHECKSUM_MISMATCH",
            Self::RateLimited => "RATE_LIMITED",
            Self::ProviderUnavailable => "PROVIDER_UNAVAILABLE",
            Self::NetworkError => "NETWORK_ERROR",
            Self::StorageError => "STORAGE_ERROR",
            Self::InternalError => "INTERNAL_ERROR",
            Self::Interrupted => "INTERRUPTED",
        }
    }

    /// Returns the process exit code required by the CLI contract.
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::InvalidRequest => 2,
            Self::AuthRequired | Self::InvalidToken | Self::TokenExpired => 10,
            Self::Forbidden => 11,
            Self::NotFound => 12,
            Self::Conflict => 13,
            Self::PlanLimit => 14,
            Self::OperationUnsupported => 15,
            Self::NetworkError => 20,
            Self::ProviderUnavailable => 21,
            Self::UploadIncomplete | Self::ChecksumMismatch | Self::StorageError => 22,
            Self::RateLimited => 23,
            Self::InternalError => 70,
            Self::Interrupted => 130,
        }
    }

    /// Returns calm, stable copy with an actionable recovery step.
    #[must_use]
    pub const fn default_message(self) -> &'static str {
        match self {
            Self::InvalidRequest => "That request isn't valid. Check the command and try again.",
            Self::AuthRequired => "Sign in with blobyard login.",
            Self::InvalidToken => "Your session isn't valid. Sign in again.",
            Self::TokenExpired => "Your session expired. Sign in again.",
            Self::Forbidden => "You don't have access to do that.",
            Self::NotFound => "That item couldn't be found. Check the name and try again.",
            Self::Conflict => {
                "That action conflicts with the current state. Refresh and try again."
            }
            Self::PlanLimit => {
                "Your current plan doesn't allow this. Review your plan and try again."
            }
            Self::OperationUnsupported => {
                "This operation isn't available on the selected Blob Yard deployment."
            }
            Self::UploadIncomplete => "The upload is incomplete. Resume it or start again.",
            Self::ChecksumMismatch => {
                "The uploaded file failed its integrity check. Upload it again."
            }
            Self::RateLimited => "Too many requests were made. Wait a moment and try again.",
            Self::ProviderUnavailable => {
                "Blobyard can't complete this right now. Try again shortly."
            }
            Self::NetworkError => "Blobyard couldn't connect. Check your connection and try again.",
            Self::StorageError => "The file transfer couldn't finish. Try again.",
            Self::InternalError => "Blobyard couldn't complete that. Try again or contact support.",
            Self::Interrupted => "The operation was cancelled. Run the command again when ready.",
        }
    }
}

impl Display for ErrorCode {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A redaction-safe error with an optional server request identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlobyardError {
    code: ErrorCode,
    message: String,
    request_id: Option<String>,
}

impl BlobyardError {
    /// Creates an error without a request identifier.
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            request_id: None,
        }
    }

    /// Creates an error using the stable message for its code.
    #[must_use]
    pub fn from_code(code: ErrorCode) -> Self {
        Self::new(code, code.default_message())
    }

    /// Attaches the server request identifier used for support and audit lookup.
    #[must_use]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// Returns the stable error code.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// Returns the user-facing, redaction-safe message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the optional server request identifier.
    #[must_use]
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }
}

impl Display for BlobyardError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}] {}", self.code, self.message)
    }
}

impl Error for BlobyardError {}
