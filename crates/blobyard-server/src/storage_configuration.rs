use crate::{RuntimeStorage, ServerError};
use blobyard_core::SecretString;
use blobyard_storage_filesystem::FilesystemStorage;
use blobyard_storage_s3::{S3Credentials, S3Storage, S3StorageConfig};
use std::path::Path;
use std::sync::Arc;

/// Standalone object-storage selection shared by serving and operator commands.
#[derive(Clone, Debug, Default)]
pub enum StorageConfiguration {
    /// Store bytes beneath the standalone data directory.
    #[default]
    Filesystem,
    /// Store bytes in an S3-compatible bucket while staging transfers locally.
    S3(S3RuntimeConfiguration),
}

/// Validated secret-bearing configuration for one S3-compatible backend.
#[derive(Clone, Debug)]
pub struct S3RuntimeConfiguration {
    endpoint: String,
    region: String,
    bucket: String,
    prefix: Option<String>,
    force_path_style: bool,
    credentials: S3Credentials,
}

impl S3RuntimeConfiguration {
    /// Creates an S3 runtime configuration. Provider validation occurs before connection.
    #[must_use]
    pub const fn new(
        endpoint: String,
        region: String,
        bucket: String,
        access_key_id: SecretString,
        secret_access_key: SecretString,
        session_token: Option<SecretString>,
    ) -> Self {
        Self {
            endpoint,
            region,
            bucket,
            prefix: None,
            force_path_style: false,
            credentials: S3Credentials::new(access_key_id, secret_access_key, session_token),
        }
    }

    /// Selects a relative provider key prefix.
    #[must_use]
    pub fn with_prefix(mut self, prefix: Option<String>) -> Self {
        self.prefix = prefix;
        self
    }

    /// Selects path-style addressing for `MinIO` and compatible providers.
    #[must_use]
    pub const fn with_force_path_style(mut self, enabled: bool) -> Self {
        self.force_path_style = enabled;
        self
    }

    pub(crate) fn open(&self, data_directory: &Path) -> Result<S3Storage, ServerError> {
        let config = S3StorageConfig::new(
            &self.endpoint,
            &self.region,
            &self.bucket,
            self.credentials.clone(),
            data_directory.join("staging/s3"),
        )
        .and_then(|config| config.with_prefix(self.prefix.as_deref()))
        .map(|config| config.with_force_path_style(self.force_path_style))
        .map_err(|_error| ServerError::Storage)?;
        S3Storage::open(&config).map_err(|_error| ServerError::Storage)
    }
}

impl StorageConfiguration {
    pub(crate) fn open(
        &self,
        data_directory: &Path,
    ) -> Result<Arc<dyn RuntimeStorage>, ServerError> {
        match self {
            Self::Filesystem => FilesystemStorage::open(&data_directory.join("objects"))
                .map(|storage| Arc::new(storage) as Arc<dyn RuntimeStorage>)
                .map_err(|_error| ServerError::Storage),
            Self::S3(config) => config
                .open(data_directory)
                .map(|storage| Arc::new(storage) as Arc<dyn RuntimeStorage>),
        }
    }
}

#[cfg(test)]
#[path = "storage_configuration_tests.rs"]
mod tests;
