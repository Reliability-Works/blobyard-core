use crate::{ApiCallError, ApiRequest, RawResponse, RetryAdvice, Transport};
use blobyard_core::{BlobyardError, ErrorCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::Arc;

/// Deployment capabilities enforced before transport execution.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ApiDeployment {
    /// Blob Yard Cloud, including hosted extension operations.
    #[default]
    Cloud,
    /// A standalone deployment that implements only the core contract.
    SelfHosted,
}

/// A decoded successful API response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiSuccess<T> {
    data: T,
    request_id: String,
}

impl<T> ApiSuccess<T> {
    /// Returns the decoded response body.
    #[must_use]
    pub const fn data(&self) -> &T {
        &self.data
    }

    /// Consumes the response and returns its body.
    #[must_use]
    pub fn into_data(self) -> T {
        self.data
    }

    /// Returns the request identifier for verbose support diagnostics.
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
}

/// Typed Blobyard API client over a one-shot transport.
#[derive(Clone)]
pub struct ApiClient {
    transport: Arc<dyn Transport>,
    deployment: ApiDeployment,
}

impl ApiClient {
    /// Creates a client from a transport seam.
    #[must_use]
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self {
            transport,
            deployment: ApiDeployment::Cloud,
        }
    }

    /// Creates a client that enforces the selected deployment's contract.
    #[must_use]
    pub fn for_deployment(transport: Arc<dyn Transport>, deployment: ApiDeployment) -> Self {
        Self {
            transport,
            deployment,
        }
    }

    /// Sends exactly one request and decodes the required response envelope.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe protocol, transport, or API error. The error
    /// carries retry guidance, but this method never retries implicitly.
    pub async fn execute<T>(&self, request: ApiRequest) -> Result<ApiSuccess<T>, ApiCallError>
    where
        T: DeserializeOwned,
    {
        let availability = request.endpoint().availability();
        let unavailable = match self.deployment {
            ApiDeployment::Cloud => matches!(
                availability,
                crate::OperationAvailability::SelfHostedOnly
                    | crate::OperationAvailability::Internal
            ),
            ApiDeployment::SelfHosted => matches!(
                availability,
                crate::OperationAvailability::HostedExtension
                    | crate::OperationAvailability::Internal
            ),
        };
        if unavailable {
            return Err(ApiCallError::new(
                BlobyardError::from_code(ErrorCode::OperationUnsupported),
                RetryAdvice::Never,
            ));
        }
        let response = self.transport.send(&request).await?;
        decode_response(&request, &response)
    }
}

impl std::fmt::Debug for ApiClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ApiClient").finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiErrorBody>,
    request_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApiErrorBody {
    code: ErrorCode,
    message: String,
    #[serde(default)]
    details: Option<Value>,
}

fn decode_response<T>(
    request: &ApiRequest,
    response: &RawResponse,
) -> Result<ApiSuccess<T>, ApiCallError>
where
    T: DeserializeOwned,
{
    let advice = request.retry_advice(response.status(), response.retry_after());
    let header_id = valid_request_id(response.request_id()).ok_or_else(|| {
        protocol_error(
            None,
            advice,
            "The service response omitted its request identifier.",
        )
    })?;
    let envelope = serde_json::from_slice::<Envelope<T>>(response.body()).map_err(|_| {
        protocol_error(
            Some(header_id),
            advice,
            "The service returned an unreadable response. Try again shortly.",
        )
    })?;
    if envelope.request_id != header_id {
        return Err(protocol_error(
            Some(header_id),
            advice,
            "The service response identifiers didn't match. Try again shortly.",
        ));
    }
    classify_envelope(response.status(), envelope, advice)
}

fn classify_envelope<T>(
    status: u16,
    envelope: Envelope<T>,
    advice: RetryAdvice,
) -> Result<ApiSuccess<T>, ApiCallError> {
    let successful_status = (200..300).contains(&status);
    match (
        successful_status,
        envelope.ok,
        envelope.data,
        envelope.error,
    ) {
        (true, true, Some(data), None) => Ok(ApiSuccess {
            data,
            request_id: envelope.request_id,
        }),
        (false, false, None, Some(error)) => {
            let _ = (error.message, error.details);
            let error = BlobyardError::from_code(error.code).with_request_id(envelope.request_id);
            Err(ApiCallError::new(error, advice))
        }
        _ => Err(protocol_error(
            Some(&envelope.request_id),
            advice,
            "The service returned an inconsistent response. Try again shortly.",
        )),
    }
}

fn valid_request_id(value: Option<&str>) -> Option<&str> {
    value.filter(|id| {
        !id.is_empty() && id.len() <= 128 && id.bytes().all(|byte| byte.is_ascii_graphic())
    })
}

fn protocol_error(
    request_id: Option<&str>,
    advice: RetryAdvice,
    message: &'static str,
) -> ApiCallError {
    let mut error = BlobyardError::new(ErrorCode::ProviderUnavailable, message);
    if let Some(request_id) = request_id {
        error = error.with_request_id(request_id);
    }
    ApiCallError::new(error, advice)
}
