//! Single-node HTTP runtime for self-hosted Blob Yard installations.

#[cfg(test)]
extern crate self as blobyard_server;

#[doc(hidden)]
pub mod api;
#[doc(hidden)]
pub mod api_ci_exchange;
#[doc(hidden)]
pub mod api_ci_trusts;
#[doc(hidden)]
pub mod api_cli_sessions;
#[doc(hidden)]
pub mod api_tokens;
#[doc(hidden)]
pub mod api_workspace_rename;
mod application;
#[doc(hidden)]
pub mod audit;
#[doc(hidden)]
pub mod auth;
#[cfg(test)]
#[doc(hidden)]
pub mod contract_test_support;
#[doc(hidden)]
pub mod download_io;
#[doc(hidden)]
pub mod error;
#[doc(hidden)]
pub mod expiry;
mod hosted_migration;
#[doc(hidden)]
pub mod inbox_rate;
#[doc(hidden)]
pub mod inbox_upload_auth;
#[doc(hidden)]
pub mod inbox_uploads;
#[doc(hidden)]
pub mod inboxes;
#[doc(hidden)]
pub mod lifecycle;
#[doc(hidden)]
pub mod objects;
#[doc(hidden)]
pub mod oidc;
#[doc(hidden)]
pub mod previews;
mod reconciliation;
mod recovery;
#[cfg(test)]
#[doc(hidden)]
pub mod repository_fault_tests;
#[doc(hidden)]
pub mod response;
#[doc(hidden)]
pub mod retention;
#[doc(hidden)]
pub mod shares;
#[doc(hidden)]
pub mod site_contracts;
#[doc(hidden)]
pub mod slug;
mod storage_configuration;
#[cfg(test)]
#[path = "test_support/storage_get_macro.rs"]
mod storage_get_macro;
#[cfg(test)]
#[path = "test_support/storage_multipart_macro.rs"]
mod storage_multipart_macro;
#[cfg(test)]
#[path = "test_support/storage_part_macro.rs"]
mod storage_part_macro;
#[cfg(test)]
#[path = "test_support/storage_put_macro.rs"]
mod storage_put_macro;
#[cfg(test)]
#[doc(hidden)]
pub mod test_support;
#[doc(hidden)]
pub mod transfer_grants;
#[doc(hidden)]
pub mod transfer_io;
#[doc(hidden)]
pub mod transfer_multipart;
#[doc(hidden)]
pub mod transfer_multipart_http;
#[doc(hidden)]
pub mod transfers;
#[doc(hidden)]
pub mod transfers_operations;
#[doc(hidden)]
pub mod yard_cleanup;
#[doc(hidden)]
pub mod yards;

#[cfg(any(test, feature = "test-seams"))]
#[doc(hidden)]
pub use application::test_seams as application_test_seams;
pub use application::{
    InitializedServer, enforce_retention, enforce_retention_with_storage, initialize,
    initialize_with_origin, initialize_with_origins, serve_until, serve_until_with_storage,
    show_new_token,
};
pub use error::ServerError;
pub use hosted_migration::{HostedMigrationError, HostedMigrationOptions, migrate_from_hosted};
pub use reconciliation::reconcile_data_directory;
pub use recovery::{
    RecoveryError, backup_data_directory, restore_data_directory, rollback_preflight,
    upgrade_preflight,
};
pub use storage_configuration::{S3RuntimeConfiguration, StorageConfiguration};

/// Converts the HTTP server's terminal I/O result into the operator entry point's error type.
///
/// # Errors
///
/// Returns the original I/O failure when the server terminates unsuccessfully.
#[doc(hidden)]
pub fn server_result(result: std::io::Result<()>) -> Result<(), Box<dyn std::error::Error>> {
    result.map_err(Into::into)
}

pub(crate) trait RuntimeStorage:
    blobyard_contract::ObjectStorage + blobyard_contract::ObjectStorageInventory
{
}

impl<T> RuntimeStorage for T where
    T: blobyard_contract::ObjectStorage + blobyard_contract::ObjectStorageInventory
{
}

pub(crate) trait Repository:
    blobyard_contract::MetadataRepository
    + blobyard_contract::CredentialRepository
    + blobyard_contract::CiRepository
    + blobyard_contract::TransferRepository
    + blobyard_contract::LifecycleRepository
    + blobyard_contract::SharingRepository
    + blobyard_contract::InboxRepository
    + blobyard_contract::PreviewRepository
    + blobyard_contract::WebYardRepository
{
}

impl<T> Repository for T where
    T: blobyard_contract::MetadataRepository
        + blobyard_contract::CredentialRepository
        + blobyard_contract::CiRepository
        + blobyard_contract::TransferRepository
        + blobyard_contract::LifecycleRepository
        + blobyard_contract::SharingRepository
        + blobyard_contract::InboxRepository
        + blobyard_contract::PreviewRepository
        + blobyard_contract::WebYardRepository
{
}

fn normalize_origin(value: &str) -> Result<String, ServerError> {
    let parsed = url::Url::parse(value).map_err(|_error| ServerError::PublicOrigin)?;
    let valid = matches!(parsed.scheme(), "http" | "https")
        && parsed.host_str().is_some()
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.path() == "/"
        && parsed.query().is_none()
        && parsed.fragment().is_none();
    if valid {
        Ok(parsed.as_str().trim_end_matches('/').to_owned())
    } else {
        Err(ServerError::PublicOrigin)
    }
}

fn runtime_secret(
    data_directory: &std::path::Path,
) -> Result<blobyard_core::SecretString, ServerError> {
    let path = data_directory.join("runtime.secret");
    match std::fs::read_to_string(&path) {
        Ok(value) => {
            blobyard_core::SecretString::new(value).map_err(|_error| ServerError::Initialization)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_runtime_secret(data_directory, &path)
        }
        Err(_error) => Err(ServerError::DataDirectory),
    }
}

fn create_runtime_secret(
    data_directory: &std::path::Path,
    path: &std::path::Path,
) -> Result<blobyard_core::SecretString, ServerError> {
    use blobyard_core::GeneratedSecretKind;
    use std::io::Write as _;

    let secret = auth::generate_token(GeneratedSecretKind::RuntimeSecret);
    let mut temporary = tempfile::NamedTempFile::new_in(data_directory)
        .map_err(|_error| ServerError::DataDirectory)?;
    let written = temporary
        .write_all(secret.expose_secret().as_bytes())
        .and_then(|()| temporary.flush())
        .and_then(|()| temporary.as_file().sync_all());
    persist_runtime_secret(data_directory, path, temporary, secret, written)
}

fn persist_runtime_secret(
    data_directory: &std::path::Path,
    path: &std::path::Path,
    temporary: tempfile::NamedTempFile,
    secret: blobyard_core::SecretString,
    written: std::io::Result<()>,
) -> Result<blobyard_core::SecretString, ServerError> {
    written.map_err(|_error| ServerError::DataDirectory)?;
    match temporary.persist_noclobber(path) {
        Ok(_file) => Ok(secret),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            runtime_secret(data_directory)
        }
        Err(_error) => Err(ServerError::DataDirectory),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn server_results_preserve_success_and_failure() {
        assert!(super::server_result(Ok(())).is_ok());
        assert!(super::server_result(Err(std::io::Error::other("fixture"))).is_err());
    }
}
