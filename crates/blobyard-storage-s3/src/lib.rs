//! S3-compatible durable object storage for standalone Blob Yard.

#[doc(hidden)]
pub mod client;
mod client_inventory;
mod client_multipart;
#[doc(hidden)]
pub mod client_objects;
mod config;
#[doc(hidden)]
pub mod error;
mod inventory;
mod locator;
mod metadata;
mod multipart;
mod objects;
#[cfg(test)]
#[doc(hidden)]
pub mod replay;
mod runtime;
#[doc(hidden)]
pub mod signing;
mod staging;
#[doc(hidden)]
pub mod transport;
#[doc(hidden)]
pub mod xml;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub use config::{S3Credentials, S3StorageConfig};

use blobyard_contract::{ObjectStorage, StorageError, StorageKey, StorageMetadata};
use client::S3Client;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use transport::RequestBody;

pub(crate) type BodyFuture =
    Pin<Box<dyn Future<Output = Result<RequestBody, StorageError>> + Send>>;
pub(crate) type BodyBuilder = fn(PathBuf) -> BodyFuture;

struct MultipartLocator {
    key: StorageKey,
    upload_id: String,
}

struct RuntimeBridge {
    handle: tokio::runtime::Handle,
    shutdown: Option<std::sync::mpsc::Sender<()>>,
}

struct StagedUpload {
    temporary: tempfile::NamedTempFile,
    metadata: StorageMetadata,
}

struct StagedRead {
    file: std::fs::File,
    _temporary: tempfile::NamedTempFile,
}

/// An S3-compatible object adapter with bounded local transfer staging.
#[derive(Clone)]
pub struct S3Storage {
    client: S3Client,
    runtime: Arc<RuntimeBridge>,
    bucket: String,
    prefix: Option<String>,
    staging_directory: PathBuf,
    body_builder: BodyBuilder,
}

impl S3Storage {
    /// Opens the adapter and its dedicated asynchronous SDK runtime.
    ///
    /// # Errors
    ///
    /// Returns a stable storage error when configuration or local staging cannot be initialized.
    pub fn open(config: &S3StorageConfig) -> Result<Self, StorageError> {
        std::fs::create_dir_all(config.staging_directory())
            .map_err(|_error| StorageError::Unavailable)?;
        let runtime = RuntimeBridge::start();
        let client = config.client();
        Self::from_config_results(config, runtime, client)
    }

    fn from_config_results(
        config: &S3StorageConfig,
        runtime: Result<RuntimeBridge, StorageError>,
        client: Result<S3Client, StorageError>,
    ) -> Result<Self, StorageError> {
        Ok(Self {
            client: client?,
            runtime: Arc::new(runtime?),
            bucket: config.bucket().to_owned(),
            prefix: config.prefix().map(str::to_owned),
            staging_directory: config.staging_directory().to_path_buf(),
            body_builder: Self::byte_stream,
        })
    }

    fn provider_key(&self, key: &StorageKey) -> String {
        self.prefix.as_ref().map_or_else(
            || key.as_str().to_owned(),
            |prefix| format!("{prefix}/{}", key.as_str()),
        )
    }

    #[cfg(test)]
    fn from_test_client_with_body_builder(
        client: S3Client,
        bucket: &str,
        prefix: Option<&str>,
        staging_directory: PathBuf,
        body_builder: BodyBuilder,
    ) -> Result<Self, StorageError> {
        std::fs::create_dir_all(&staging_directory).map_err(|_error| StorageError::Unavailable)?;
        Self::from_test_parts(
            client,
            bucket,
            prefix,
            staging_directory,
            body_builder,
            RuntimeBridge::start(),
        )
    }

    #[cfg(test)]
    fn from_test_parts(
        client: S3Client,
        bucket: &str,
        prefix: Option<&str>,
        staging_directory: PathBuf,
        body_builder: BodyBuilder,
        runtime: Result<RuntimeBridge, StorageError>,
    ) -> Result<Self, StorageError> {
        Ok(Self {
            client,
            runtime: Arc::new(runtime?),
            bucket: bucket.to_owned(),
            prefix: prefix.map(str::to_owned),
            staging_directory,
            body_builder,
        })
    }
}

impl std::fmt::Debug for S3Storage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("S3Storage")
            .field("bucket", &self.bucket)
            .field("prefix", &self.prefix)
            .field("staging_directory", &self.staging_directory)
            .finish_non_exhaustive()
    }
}

impl ObjectStorage for S3Storage {
    fn put(
        &self,
        key: &StorageKey,
        source: &mut dyn std::io::Read,
        expected: Option<&blobyard_contract::ObjectChecksum>,
    ) -> Result<blobyard_contract::StorageMetadata, StorageError> {
        self.put_object(key, source, expected)
    }

    fn get(
        &self,
        key: &StorageKey,
        range: Option<blobyard_contract::ByteRange>,
    ) -> Result<blobyard_contract::StorageRead, StorageError> {
        self.get_object(key, range)
    }

    fn head(&self, key: &StorageKey) -> Result<blobyard_contract::StorageMetadata, StorageError> {
        self.head_object(key)
    }

    fn delete(&self, key: &StorageKey) -> Result<(), StorageError> {
        self.delete_object(key)
    }

    fn begin_multipart(
        &self,
        key: &StorageKey,
        expected: &blobyard_contract::StorageMetadata,
    ) -> Result<blobyard_contract::MultipartId, StorageError> {
        self.create_multipart(key, expected)
    }

    fn put_part(
        &self,
        upload: &blobyard_contract::MultipartId,
        number: u32,
        source: &mut dyn std::io::Read,
    ) -> Result<blobyard_contract::MultipartPart, StorageError> {
        self.upload_part(upload, number, source)
    }

    fn complete_multipart(
        &self,
        upload: &blobyard_contract::MultipartId,
        parts: &[blobyard_contract::MultipartPart],
    ) -> Result<blobyard_contract::StorageMetadata, StorageError> {
        self.commit_multipart(upload, parts)
    }

    fn abort_multipart(&self, upload: &blobyard_contract::MultipartId) -> Result<(), StorageError> {
        self.cancel_multipart(upload)
    }
}
