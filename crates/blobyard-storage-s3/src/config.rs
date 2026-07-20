use crate::client::S3Client;
use crate::transport::{ReqwestTransport, S3Transport};
use blobyard_contract::{StorageError, StorageKey};
use blobyard_core::SecretString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use url::Url;

/// Static S3 credentials supplied by the standalone operator.
#[derive(Clone, Debug)]
pub struct S3Credentials {
    access_key_id: SecretString,
    secret_access_key: SecretString,
    session_token: Option<SecretString>,
}

impl S3Credentials {
    /// Creates credentials that are kept redacted by Blob Yard value types.
    #[must_use]
    pub const fn new(
        access_key_id: SecretString,
        secret_access_key: SecretString,
        session_token: Option<SecretString>,
    ) -> Self {
        Self {
            access_key_id,
            secret_access_key,
            session_token,
        }
    }

    pub(crate) fn access_key_id(&self) -> &str {
        self.access_key_id.expose_secret()
    }

    pub(crate) fn secret_access_key(&self) -> &str {
        self.secret_access_key.expose_secret()
    }

    pub(crate) fn session_token(&self) -> Option<&str> {
        self.session_token.as_ref().map(SecretString::expose_secret)
    }
}

/// Validated connection and staging configuration for S3-compatible storage.
#[derive(Clone, Debug)]
pub struct S3StorageConfig {
    endpoint: Url,
    region: String,
    bucket: String,
    prefix: Option<String>,
    credentials: S3Credentials,
    force_path_style: bool,
    staging_directory: PathBuf,
}

impl S3StorageConfig {
    /// Validates an S3 endpoint, bucket, region, credentials, and local staging directory.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidInput`] for unsafe or malformed configuration.
    pub fn new(
        endpoint: &str,
        region: &str,
        bucket: &str,
        credentials: S3Credentials,
        staging_directory: PathBuf,
    ) -> Result<Self, StorageError> {
        let endpoint = validate_endpoint(endpoint)?;
        validate_name(region, 128)?;
        validate_name(bucket, 255)?;
        if staging_directory.as_os_str().is_empty() {
            return Err(StorageError::InvalidInput);
        }
        Ok(Self {
            endpoint,
            region: region.to_owned(),
            bucket: bucket.to_owned(),
            prefix: None,
            credentials,
            force_path_style: false,
            staging_directory,
        })
    }

    /// Adds a validated relative key prefix beneath the configured bucket.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidInput`] for an unsafe prefix.
    pub fn with_prefix(mut self, prefix: Option<&str>) -> Result<Self, StorageError> {
        self.prefix = prefix
            .map(|value| StorageKey::new(value.to_owned()).map(|key| key.as_str().to_owned()))
            .transpose()?;
        Ok(self)
    }

    /// Selects path-style addressing, required by `MinIO` and some compatible providers.
    #[must_use]
    pub const fn with_force_path_style(mut self, enabled: bool) -> Self {
        self.force_path_style = enabled;
        self
    }

    pub(crate) fn client(&self) -> Result<S3Client, StorageError> {
        self.client_from_transport(ReqwestTransport::new().map(|value| {
            let transport: Arc<dyn S3Transport> = Arc::new(value);
            transport
        }))
    }

    pub(crate) fn client_from_transport(
        &self,
        transport: Result<Arc<dyn S3Transport>, StorageError>,
    ) -> Result<S3Client, StorageError> {
        Ok(S3Client::new(
            transport?,
            self.endpoint.clone(),
            self.region.clone(),
            self.bucket.clone(),
            self.credentials.clone(),
            self.force_path_style,
        ))
    }

    pub(crate) fn bucket(&self) -> &str {
        &self.bucket
    }

    pub(crate) fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    pub(crate) fn staging_directory(&self) -> &Path {
        &self.staging_directory
    }
}

fn validate_endpoint(value: &str) -> Result<Url, StorageError> {
    let endpoint = Url::parse(value).map_err(|_error| StorageError::InvalidInput)?;
    let valid = matches!(endpoint.scheme(), "http" | "https")
        && endpoint.host_str().is_some()
        && endpoint.username().is_empty()
        && endpoint.password().is_none()
        && endpoint.path() == "/"
        && endpoint.query().is_none()
        && endpoint.fragment().is_none();
    if valid {
        Ok(endpoint)
    } else {
        Err(StorageError::InvalidInput)
    }
}

fn validate_name(value: &str, max: usize) -> Result<(), StorageError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        Err(StorageError::InvalidInput)
    } else {
        Ok(())
    }
}
