use crate::ServerError;
use openidconnect::{AsyncHttpClient, HttpClientError, HttpRequest, HttpResponse};
use std::{future::Future, pin::Pin, time::Duration};

/// Maximum byte size of one OIDC provider response body.
///
/// Discovery, JWKS, token, and `UserInfo` documents are small JSON payloads. The bound rejects an
/// oversized declared content length before reading and stops a streamed body at the same limit,
/// so a misbehaving provider cannot buffer unbounded memory inside the server process.
const MAXIMUM_RESPONSE_BYTES: usize = 4 * 1_024 * 1_024;

pub(super) const INSECURE_URL_MESSAGE: &str = "OIDC provider URL fails the secure endpoint policy";
pub(super) const OVERSIZED_MESSAGE: &str = "OIDC provider response exceeds the byte limit";

pub(super) struct OidcHttpClient {
    inner: reqwest::Client,
}

impl OidcHttpClient {
    pub(super) fn new() -> Result<Self, ServerError> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()
            .map(|inner| Self { inner })
            .map_err(|_error| ServerError::OidcDiscovery)
    }
}

/// Applies the secure provider endpoint policy: HTTPS everywhere, HTTP only for loopback hosts,
/// and no credentials or fragments. Every provider URL is checked before any request executes.
pub(super) fn secure_endpoint(value: &url::Url) -> bool {
    let loopback = value.host().is_some_and(|host| match host {
        url::Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
        url::Host::Ipv4(address) => address.is_loopback(),
        url::Host::Ipv6(address) => address.is_loopback(),
    });
    (value.scheme() == "https" || (value.scheme() == "http" && loopback))
        && value.username().is_empty()
        && value.password().is_none()
        && value.fragment().is_none()
}

impl<'client> AsyncHttpClient<'client> for OidcHttpClient {
    type Error = HttpClientError<reqwest::Error>;
    type Future =
        Pin<Box<dyn Future<Output = Result<HttpResponse, Self::Error>> + Send + Sync + 'client>>;

    fn call(&'client self, request: HttpRequest) -> Self::Future {
        Box::pin(async move {
            let request: reqwest::Request = request.try_into().map_err(Box::new)?;
            if !secure_endpoint(request.url()) {
                return Err(HttpClientError::Other(INSECURE_URL_MESSAGE.to_owned()));
            }
            let response = self.inner.execute(request).await.map_err(Box::new)?;
            let mut builder = axum::http::Response::builder()
                .status(response.status())
                .version(response.version());
            for (name, value) in response.headers() {
                builder = builder.header(name, value);
            }
            builder
                .body(bounded_body(response).await?)
                .map_err(HttpClientError::Http)
        })
    }
}

async fn bounded_body(
    mut response: reqwest::Response,
) -> Result<Vec<u8>, HttpClientError<reqwest::Error>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAXIMUM_RESPONSE_BYTES as u64)
    {
        return Err(HttpClientError::Other(OVERSIZED_MESSAGE.to_owned()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(Box::new)? {
        if body.len() + chunk.len() > MAXIMUM_RESPONSE_BYTES {
            return Err(HttpClientError::Other(OVERSIZED_MESSAGE.to_owned()));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
