use blobyard_core::{BlobyardError, ErrorCode};
use std::time::Duration;
use url::{Host, Url};

/// The production base URL for Blobyard's versioned API.
pub const DEFAULT_API_BASE_URL: &str = "https://api.blobyard.com/v1";

/// Default whole-request timeout.
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Default connection-establishment timeout.
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Validated configuration for a Blobyard API client.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiClientConfig {
    api_base_url: Url,
    request_timeout: Duration,
    connect_timeout: Duration,
}

impl ApiClientConfig {
    /// Validates and normalizes an API base URL.
    ///
    /// # Errors
    ///
    /// Returns `INVALID_REQUEST` when the URL is not a clean HTTP(S) API
    /// origin. Plain HTTP is limited to loopback development endpoints.
    pub fn new(api_base_url: impl AsRef<str>) -> Result<Self, BlobyardError> {
        let mut parsed = Url::parse(api_base_url.as_ref()).map_err(|_| invalid_api_url())?;
        validate_url(&parsed)?;
        parsed.set_query(None);
        parsed.set_fragment(None);
        parsed.set_path("/v1");
        Ok(Self {
            api_base_url: parsed,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        })
    }

    /// Overrides bounded transport timeouts.
    ///
    /// # Errors
    ///
    /// Returns `INVALID_REQUEST` when either timeout is zero or the connect
    /// timeout is longer than the whole-request timeout.
    pub fn with_timeouts(
        mut self,
        request_timeout: Duration,
        connect_timeout: Duration,
    ) -> Result<Self, BlobyardError> {
        let invalid = request_timeout.is_zero()
            || connect_timeout.is_zero()
            || connect_timeout > request_timeout;
        if invalid {
            return Err(BlobyardError::new(
                ErrorCode::InvalidRequest,
                "API timeouts aren't valid. Use positive values with connect no longer than request.",
            ));
        }
        self.request_timeout = request_timeout;
        self.connect_timeout = connect_timeout;
        Ok(self)
    }

    /// Returns the normalized API base URL.
    #[must_use]
    pub fn api_base_url(&self) -> &str {
        self.api_base_url.as_str().trim_end_matches('/')
    }

    /// Returns the whole-request timeout.
    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    /// Returns the connection-establishment timeout.
    #[must_use]
    pub const fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    pub(crate) fn endpoint_url(&self, endpoint_path: &str) -> Url {
        let mut url = self.api_base_url.clone();
        url.set_path(endpoint_path);
        url
    }
}

fn validate_url(url: &Url) -> Result<(), BlobyardError> {
    let valid_scheme = url.scheme() == "https"
        || (url.scheme() == "http" && url.host().is_some_and(|host| host_is_loopback(&host)));
    let valid_authority =
        url.host().is_some() && url.username().is_empty() && url.password().is_none();
    let valid_tail = url.query().is_none()
        && url.fragment().is_none()
        && matches!(url.path(), "" | "/" | "/v1" | "/v1/");
    if valid_scheme && valid_authority && valid_tail {
        Ok(())
    } else {
        Err(invalid_api_url())
    }
}

/// Returns whether a parsed URL host is a local loopback target.
#[must_use]
pub fn host_is_loopback(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => *domain == "localhost",
        Host::Ipv4(address) => address.is_loopback(),
        Host::Ipv6(address) => address.is_loopback(),
    }
}

fn invalid_api_url() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InvalidRequest,
        "API URL must be HTTPS, or HTTP on a loopback development host.",
    )
}
