use crate::{BlobyardError, ErrorCode, secret::SecretString};
use url::{Host, Url};

const MAXIMUM_DNS_LABEL_BYTES: usize = 63;
const MAXIMUM_DNS_NAME_BYTES: usize = 253;
const MAXIMUM_WEB_YARD_ROOT_BYTES: usize = MAXIMUM_DNS_NAME_BYTES - MAXIMUM_DNS_LABEL_BYTES - 1;

/// The isolated public origin used by Blob Yard Cloud Web Yards.
pub const CLOUD_WEB_YARD_ORIGIN: &str = "https://blobyard.app";

/// A trusted root origin whose first-level subdomains serve public Web Yards.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebYardOrigin {
    scheme: String,
    domain: String,
    port: Option<u16>,
    authority: String,
    serialized: String,
}

impl WebYardOrigin {
    /// Validates and normalizes a Web Yard root origin.
    ///
    /// # Errors
    ///
    /// Returns `INVALID_REQUEST` unless the value is a clean HTTPS domain origin. Plain HTTP is
    /// accepted only for `localhost` development. Paths, credentials, queries, fragments, and IP
    /// literals are rejected because Web Yards are served from first-level subdomains.
    pub fn new(value: impl AsRef<str>) -> Result<Self, BlobyardError> {
        let parsed = Url::parse(value.as_ref()).map_err(|_error| invalid_origin())?;
        let domain = match parsed.host() {
            Some(Host::Domain(domain)) => domain.to_owned(),
            _ => return Err(invalid_origin()),
        };
        let valid_scheme =
            parsed.scheme() == "https" || (parsed.scheme() == "http" && domain == "localhost");
        let valid_authority = parsed.username().is_empty() && parsed.password().is_none();
        let valid_tail = matches!(parsed.path(), "" | "/")
            && parsed.query().is_none()
            && parsed.fragment().is_none();
        if !valid_scheme || !valid_authority || !valid_tail || !valid_root_domain(&domain) {
            return Err(invalid_origin());
        }
        let scheme = parsed.scheme().to_owned();
        let port = parsed.port();
        let authority = serialize_authority(&domain, port);
        let serialized = serialize(&scheme, &domain, port);
        Ok(Self {
            scheme,
            domain,
            port,
            authority,
            serialized,
        })
    }

    /// Returns the normalized trusted root origin without a trailing slash.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    /// Returns the validated domain and optional port without a scheme.
    #[must_use]
    pub fn authority(&self) -> &str {
        &self.authority
    }

    /// Builds the exact root URL for one validated Web Yard host label.
    ///
    /// # Errors
    ///
    /// Returns `INVALID_REQUEST` when the label is not one safe DNS label.
    pub fn url_for(&self, host_label: &str) -> Result<String, BlobyardError> {
        if !is_valid_dns_label(host_label) {
            return Err(invalid_origin());
        }
        let host = format!("{host_label}.{}", self.domain);
        Ok(serialize(&self.scheme, &host, self.port))
    }

    /// Builds the exact secret-bearing root URL for one validated Web Yard host label.
    ///
    /// # Errors
    ///
    /// Returns `INVALID_REQUEST` when the label is not one safe DNS label.
    pub fn secret_url_for(&self, host_label: &str) -> Result<SecretString, BlobyardError> {
        let root = self.url_for(host_label)?;
        Ok(SecretString::from_validated(format!("{root}/")))
    }

    /// Returns whether a URL is the exact root URL for the supplied host label.
    #[must_use]
    pub fn matches(&self, value: &str, host_label: &str) -> bool {
        self.url_for(host_label)
            .is_ok_and(|expected| value == expected)
    }
}

fn serialize(scheme: &str, host: &str, port: Option<u16>) -> String {
    format!("{scheme}://{}", serialize_authority(host, port))
}

fn serialize_authority(host: &str, port: Option<u16>) -> String {
    port.map_or_else(|| host.to_owned(), |port| format!("{host}:{port}"))
}

/// Returns whether a value is one canonical lowercase DNS label.
#[must_use]
pub fn is_valid_dns_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAXIMUM_DNS_LABEL_BYTES
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_root_domain(value: &str) -> bool {
    value.len() <= MAXIMUM_WEB_YARD_ROOT_BYTES && value.split('.').all(is_valid_dns_label)
}

fn invalid_origin() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InvalidRequest,
        "Web Yard origin must be an HTTPS domain root, or HTTP on localhost for development.",
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::{CLOUD_WEB_YARD_ORIGIN, MAXIMUM_WEB_YARD_ROOT_BYTES, WebYardOrigin};
    use crate::ErrorCode;

    #[test]
    fn normalizes_trusted_origins_and_builds_exact_first_level_urls() {
        let cloud = WebYardOrigin::new(format!("{CLOUD_WEB_YARD_ORIGIN}/")).expect("cloud origin");
        assert_eq!(cloud.as_str(), CLOUD_WEB_YARD_ORIGIN);
        assert_eq!(cloud.authority(), "blobyard.app");
        assert_eq!(
            cloud.url_for("docs-123-main").expect("yard URL"),
            "https://docs-123-main.blobyard.app"
        );
        assert_eq!(
            cloud
                .secret_url_for("docs-123-main")
                .expect("secret yard URL")
                .expose_secret(),
            "https://docs-123-main.blobyard.app/"
        );

        let local = WebYardOrigin::new("http://localhost:8787/").expect("local origin");
        assert_eq!(local.as_str(), "http://localhost:8787");
        assert_eq!(local.authority(), "localhost:8787");
        assert!(local.matches("http://docs-123.localhost:8787", "docs-123"));
        assert!(!local.matches("http://docs-123.localhost:8787/path", "docs-123"));
    }

    #[test]
    fn rejects_ambiguous_origins_labels_and_response_urls() {
        for value in [
            "http://yards.example.com",
            "https://127.0.0.1",
            "https://user@yards.example.com",
            "https://yards.example.com/path",
            "https://yards.example.com?query=1",
            "https://yards.example.com#fragment",
            "not-an-origin",
        ] {
            let error = WebYardOrigin::new(value).expect_err(value);
            assert_eq!(error.code(), ErrorCode::InvalidRequest, "{value}");
        }

        let long_label = "a".repeat(64);
        assert!(WebYardOrigin::new(format!("https://{long_label}.example.com")).is_err());
        let oversized_root = format!("{}.{}.{}", "a".repeat(63), "b".repeat(63), "c".repeat(62));
        assert_eq!(oversized_root.len(), MAXIMUM_WEB_YARD_ROOT_BYTES + 1);
        assert!(WebYardOrigin::new(format!("https://{oversized_root}")).is_err());

        let origin = WebYardOrigin::new("https://yards.example.com").expect("origin");
        for label in [
            "",
            "-docs",
            "docs-",
            "Docs",
            "docs.example",
            &"a".repeat(64),
        ] {
            assert!(origin.url_for(label).is_err(), "{label}");
            assert!(origin.secret_url_for(label).is_err(), "{label}");
        }
        assert!(!origin.matches("https://docs.yards.example.com.evil.test", "docs"));
        assert!(!origin.matches("https://docs.yards.example.com/", "docs"));
        assert!(!origin.matches("http://docs.yards.example.com", "docs"));
    }
}
