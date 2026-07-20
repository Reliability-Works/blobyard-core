use crate::{
    ApiCallError, ApiClientConfig, ApiRequest, HttpMethod, RawResponse, Transport, TransportFuture,
};
use blobyard_core::{BlobyardError, ErrorCode};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, HeaderName, HeaderValue, RETRY_AFTER};
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 1_048_576;
const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");
const IDEMPOTENCY_KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

/// Bounded reqwest transport using rustls and no implicit retries.
#[derive(Clone, Debug)]
pub struct ReqwestTransport {
    client: reqwest::Client,
    config: ApiClientConfig,
}

impl ReqwestTransport {
    /// Builds a transport with bounded connect and whole-request timeouts.
    ///
    /// # Errors
    ///
    /// Returns a safe internal error if the HTTP client cannot be built.
    pub fn new(config: ApiClientConfig) -> Result<Self, BlobyardError> {
        map_client_result(
            reqwest::Client::builder()
                .connect_timeout(config.connect_timeout())
                .timeout(config.request_timeout())
                .user_agent(concat!("blobyard-cli/", env!("CARGO_PKG_VERSION")))
                .build(),
        )
        .map(|client| Self { client, config })
    }

    async fn send_once(&self, request: &ApiRequest) -> Result<RawResponse, ApiCallError> {
        let mut url = self.config.endpoint_url(request.endpoint().path());
        url.set_query(request.query());
        tracing::debug!("sending Blobyard API request");
        let builder = self.prepare_request(request, url);
        let response = builder.send().await.map_err(|_| transport_error(request))?;
        read_response(response, request).await
    }

    fn prepare_request(&self, request: &ApiRequest, url: url::Url) -> reqwest::RequestBuilder {
        let mut builder = self
            .client
            .request(method(request.endpoint().method()), url);
        if let Some(body) = request.body() {
            builder = builder.json(body);
        }
        if let Some(bearer) = request.bearer() {
            builder = builder.bearer_auth(bearer.expose_secret());
        }
        if let Some(key) = request.idempotency_key() {
            builder = builder.header(IDEMPOTENCY_KEY_HEADER, key);
        }
        builder
    }
}

impl Transport for ReqwestTransport {
    fn send<'a>(&'a self, request: &'a ApiRequest) -> TransportFuture<'a> {
        Box::pin(self.send_once(request))
    }
}

async fn read_response(
    response: reqwest::Response,
    request: &ApiRequest,
) -> Result<RawResponse, ApiCallError> {
    let status = response.status().as_u16();
    let request_id = response
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(header_text)
        .map(ToOwned::to_owned);
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(header_text)
        .and_then(parse_retry_after);
    reject_large_content_length(&response, request)?;
    let body = read_bounded_body(response, request).await?;
    let raw = RawResponse::new(status, request_id, body);
    Ok(match retry_after {
        Some(delay) => raw.with_retry_after(delay),
        None => raw,
    })
}

fn reject_large_content_length(
    response: &reqwest::Response,
    request: &ApiRequest,
) -> Result<(), ApiCallError> {
    let too_large = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(header_text)
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64);
    if too_large {
        Err(response_too_large(request))
    } else {
        Ok(())
    }
}

async fn read_bounded_body(
    response: reqwest::Response,
    request: &ApiRequest,
) -> Result<Vec<u8>, ApiCallError> {
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| transport_error(request))?;
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(response_too_large(request));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

const fn method(method: HttpMethod) -> reqwest::Method {
    match method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
        HttpMethod::Delete => reqwest::Method::DELETE,
    }
}

fn header_text(value: &HeaderValue) -> Option<&str> {
    value.to_str().ok()
}

fn parse_retry_after(value: &str) -> Option<Duration> {
    value.parse::<u64>().ok().map(Duration::from_secs)
}

fn transport_error(request: &ApiRequest) -> ApiCallError {
    ApiCallError::new(
        BlobyardError::from_code(ErrorCode::NetworkError),
        request.transport_failure_retry(),
    )
}

fn response_too_large(request: &ApiRequest) -> ApiCallError {
    ApiCallError::new(
        BlobyardError::new(
            ErrorCode::ProviderUnavailable,
            "The service response was too large. Narrow the request and try again.",
        ),
        request.transport_failure_retry(),
    )
}

fn map_client_result<T, E>(result: Result<T, E>) -> Result<T, BlobyardError> {
    result.map_err(|_| BlobyardError::from_code(ErrorCode::InternalError))
}

#[cfg(test)]
mod tests {
    use super::{header_text, map_client_result, method, parse_retry_after};
    use crate::HttpMethod;
    use reqwest::header::HeaderValue;
    use std::time::Duration;

    #[test]
    fn parses_supported_retry_after_seconds() {
        assert_eq!(parse_retry_after("9"), Some(Duration::from_secs(9)));
        assert_eq!(parse_retry_after("tomorrow"), None);
    }

    #[test]
    fn rejects_non_text_headers() {
        let value = HeaderValue::from_bytes(&[0xff]).ok();
        assert_eq!(value.as_ref().and_then(header_text), None);
    }

    #[test]
    fn maps_every_protocol_method() {
        assert_eq!(method(HttpMethod::Get), reqwest::Method::GET);
        assert_eq!(method(HttpMethod::Post), reqwest::Method::POST);
        assert_eq!(method(HttpMethod::Put), reqwest::Method::PUT);
        assert_eq!(method(HttpMethod::Delete), reqwest::Method::DELETE);
    }

    #[test]
    fn maps_http_client_construction_failures_safely() {
        assert_eq!(map_client_result::<(), _>(Ok::<(), ()>(())), Ok(()));
        assert_eq!(
            map_client_result::<(), _>(Err::<(), ()>(())),
            Err(blobyard_core::BlobyardError::from_code(
                blobyard_core::ErrorCode::InternalError
            ))
        );
    }
}
