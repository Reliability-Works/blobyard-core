use crate::{Endpoint, HttpMethod};
use blobyard_core::{BlobyardError, ErrorCode, SecretString, hex_digest};
use serde_json::Value;
use std::error::Error;
use std::fmt::{self, Debug, Display, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

const MAX_IDEMPOTENCY_KEY_BYTES: usize = 128;

/// Retry guidance returned to callers without performing an implicit retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryAdvice {
    /// The request must not be retried automatically.
    Never,
    /// A safe read may be retried with bounded backoff.
    SafeRequest,
    /// An idempotency-protected mutation may be retried with bounded backoff.
    IdempotentMutation,
    /// An eligible request may retry after the server-provided delay.
    After(Duration),
}

/// A fully prepared request that redacts credentials and payloads in debug output.
#[derive(Clone)]
pub struct ApiRequest {
    endpoint: Endpoint,
    query: Option<String>,
    body: Option<Value>,
    bearer: Option<SecretString>,
    idempotency_key: Option<String>,
}

impl ApiRequest {
    /// Starts a request for an endpoint.
    #[must_use]
    pub const fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            query: None,
            body: None,
            bearer: None,
            idempotency_key: None,
        }
    }

    /// Adds a query encoded by a bounded typed request model.
    #[must_use]
    pub fn with_query(mut self, query: String) -> Self {
        self.query = (!query.is_empty()).then_some(query);
        self
    }

    /// Adds JSON encoded by a bounded typed request model.
    #[must_use]
    pub fn with_json(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }

    /// Adds a bearer credential.
    #[must_use]
    pub fn with_bearer(mut self, bearer: SecretString) -> Self {
        self.bearer = Some(bearer);
        self
    }

    /// Adds a caller-controlled idempotency key.
    ///
    /// # Errors
    ///
    /// Returns `INVALID_REQUEST` for empty, overlong, or non-printable keys.
    pub fn with_idempotency_key(mut self, key: String) -> Result<Self, BlobyardError> {
        let valid = !key.is_empty()
            && key.len() <= MAX_IDEMPOTENCY_KEY_BYTES
            && key.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b':' | b'-' | b'_')
            });
        if !valid {
            return Err(BlobyardError::new(
                ErrorCode::InvalidRequest,
                "The idempotency key isn't valid. Generate a new key and try again.",
            ));
        }
        if !self.endpoint.supports_idempotency() {
            return Err(BlobyardError::new(
                ErrorCode::InvalidRequest,
                "This operation does not support idempotent replay.",
            ));
        }
        self.idempotency_key = Some(key);
        Ok(self)
    }

    /// Adds a fresh client-generated idempotency key.
    #[must_use]
    pub fn with_generated_idempotency_key(mut self) -> Self {
        if self.endpoint.supports_idempotency() {
            self.idempotency_key = Some(format!("blobyard-client-{}", uuid::Uuid::new_v4()));
        }
        self
    }

    /// Adds a deterministic idempotency key derived from a SHA-256 digest.
    #[must_use]
    pub fn with_deterministic_idempotency_key(mut self, digest: [u8; 32]) -> Self {
        if self.endpoint.supports_idempotency() {
            self.idempotency_key = Some(format!("blobyard-digest-{}", hex_digest(&digest)));
        }
        self
    }

    /// Returns the target endpoint.
    #[must_use]
    pub const fn endpoint(&self) -> Endpoint {
        self.endpoint
    }

    /// Returns the encoded query without exposing it in debug output.
    #[must_use]
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    /// Returns the JSON body without exposing it in debug output.
    #[must_use]
    pub const fn body(&self) -> Option<&Value> {
        self.body.as_ref()
    }

    /// Returns the bearer credential at the explicit transport boundary.
    #[must_use]
    pub const fn bearer(&self) -> Option<&SecretString> {
        self.bearer.as_ref()
    }

    /// Returns the idempotency key.
    #[must_use]
    pub fn idempotency_key(&self) -> Option<&str> {
        self.idempotency_key.as_deref()
    }

    /// Classifies a response for caller-controlled retry handling.
    #[must_use]
    pub fn retry_advice(&self, status: u16, retry_after: Option<Duration>) -> RetryAdvice {
        let eligible = self.endpoint.method().is_safe() || self.idempotency_key.is_some();
        if !eligible {
            return RetryAdvice::Never;
        }
        if status == 429 {
            return RetryAdvice::After(retry_after.unwrap_or(Duration::from_secs(1)));
        }
        if matches!(status, 408 | 425 | 500 | 502 | 503 | 504) {
            return retry_kind(self.endpoint.method());
        }
        RetryAdvice::Never
    }

    pub(crate) const fn transport_failure_retry(&self) -> RetryAdvice {
        if self.endpoint.method().is_safe() || self.idempotency_key.is_some() {
            retry_kind(self.endpoint.method())
        } else {
            RetryAdvice::Never
        }
    }
}

impl Debug for ApiRequest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApiRequest")
            .field("endpoint", &self.endpoint)
            .field("has_query", &self.query.is_some())
            .field("has_body", &self.body.is_some())
            .field("has_bearer", &self.bearer.is_some())
            .field("has_idempotency_key", &self.idempotency_key.is_some())
            .finish()
    }
}

/// A bounded raw HTTP response ready for envelope decoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawResponse {
    status: u16,
    request_id: Option<String>,
    retry_after: Option<Duration>,
    body: Vec<u8>,
}

impl RawResponse {
    /// Creates a raw response, primarily for transport implementations and tests.
    #[must_use]
    pub fn new(status: u16, request_id: Option<String>, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            request_id,
            retry_after: None,
            body: body.into(),
        }
    }

    /// Attaches a parsed `Retry-After` delay.
    #[must_use]
    pub const fn with_retry_after(mut self, retry_after: Duration) -> Self {
        self.retry_after = Some(retry_after);
        self
    }

    pub(crate) const fn status(&self) -> u16 {
        self.status
    }

    pub(crate) fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    pub(crate) const fn retry_after(&self) -> Option<Duration> {
        self.retry_after
    }

    pub(crate) fn body(&self) -> &[u8] {
        &self.body
    }
}

/// A transport or decoded API failure plus non-executing retry guidance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiCallError {
    error: BlobyardError,
    retry_advice: RetryAdvice,
}

impl ApiCallError {
    /// Creates an API call error.
    #[must_use]
    pub const fn new(error: BlobyardError, retry_advice: RetryAdvice) -> Self {
        Self {
            error,
            retry_advice,
        }
    }

    /// Returns the safe Blobyard error.
    #[must_use]
    pub const fn error(&self) -> &BlobyardError {
        &self.error
    }

    /// Returns retry guidance; no retry has been performed.
    #[must_use]
    pub const fn retry_advice(&self) -> RetryAdvice {
        self.retry_advice
    }

    /// Consumes the call error and returns its safe Blobyard error.
    #[must_use]
    pub fn into_error(self) -> BlobyardError {
        self.error
    }
}

impl Display for ApiCallError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.error, formatter)
    }
}

impl Error for ApiCallError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}

/// Boxed future returned by transport implementations.
pub type TransportFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RawResponse, ApiCallError>> + Send + 'a>>;

/// A one-shot HTTP transport. It never retries implicitly.
pub trait Transport: Send + Sync {
    /// Sends exactly one prepared request.
    fn send<'a>(&'a self, request: &'a ApiRequest) -> TransportFuture<'a>;
}

const fn retry_kind(method: HttpMethod) -> RetryAdvice {
    if method.is_safe() {
        RetryAdvice::SafeRequest
    } else {
        RetryAdvice::IdempotentMutation
    }
}
