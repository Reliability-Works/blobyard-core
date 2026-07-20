use crate::StorageConfiguration;
use serde::Serialize;
use std::fmt::{Display, Formatter};
use std::path::Path;

#[path = "recovery_backup.rs"]
mod backup;
#[path = "recovery_io.rs"]
mod io;
#[path = "recovery_manifest.rs"]
mod manifest;
#[path = "recovery_restore.rs"]
mod restore;
#[path = "recovery_upgrade.rs"]
mod upgrade;

/// Redaction-safe failure from a standalone recovery command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryError {
    /// The source installation is missing or unsafe to read.
    InstallationUnavailable,
    /// A backup or restore destination already exists.
    DestinationExists,
    /// The backup is malformed, unsupported, or internally inconsistent.
    InvalidBackup,
    /// Metadata could not be snapshotted or validated.
    Database,
    /// The source schema predates this binary's direct upgrade range.
    SchemaTooOld,
    /// The source schema is newer than this binary understands.
    SchemaTooNew,
    /// An upload is active, so a complete recovery point cannot be captured.
    ActiveUploads,
    /// Physical storage failed while verified bytes were copied.
    Storage,
    /// Bytes or integrity metadata did not match the recovery manifest.
    Integrity,
    /// Restore storage was not empty before import.
    StorageNotEmpty,
    /// The selected binary cannot safely roll back this schema.
    RollbackUnsafe,
    /// A private staging directory could not be persisted atomically.
    Persistence,
}

impl Display for RecoveryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InstallationUnavailable => "standalone installation is unavailable or unsafe",
            Self::DestinationExists => "recovery destination already exists",
            Self::InvalidBackup => "backup is malformed, unsupported, or inconsistent",
            Self::Database => "metadata snapshot is unavailable or corrupt",
            Self::SchemaTooOld => "metadata schema is older than this binary can upgrade",
            Self::SchemaTooNew => "metadata schema is newer than this binary supports",
            Self::ActiveUploads => "backup requires every object upload to be terminal",
            Self::Storage => "object storage is unavailable",
            Self::Integrity => "recovery bytes or integrity metadata do not match",
            Self::StorageNotEmpty => "restore requires an empty object-storage namespace",
            Self::RollbackUnsafe => "rollback binary does not exactly support the current schema",
            Self::Persistence => "recovery staging data could not be persisted atomically",
        })
    }
}

impl std::error::Error for RecoveryError {}

const fn map_repository(error: blobyard_contract::RepositoryError) -> RecoveryError {
    match error {
        blobyard_contract::RepositoryError::SchemaTooNew => RecoveryError::SchemaTooNew,
        blobyard_contract::RepositoryError::NotFound
        | blobyard_contract::RepositoryError::Conflict
        | blobyard_contract::RepositoryError::InvalidInput
        | blobyard_contract::RepositoryError::Unavailable => RecoveryError::Database,
    }
}

/// Creates one portable, checksummed recovery point in a new directory.
///
/// # Errors
///
/// Fails closed for active uploads, unsafe paths, corrupt metadata, missing bytes, checksum
/// disagreement, provider failures, or an existing destination.
pub fn backup_data_directory(
    data_directory: &Path,
    output: &Path,
    storage: &StorageConfiguration,
) -> Result<String, RecoveryError> {
    backup::create(data_directory, output, storage).and_then(|report| encode(&report))
}

/// Restores a verified backup into one absent installation and empty storage namespace.
///
/// # Errors
///
/// Fails closed before activation when the backup, schema, bytes, destination, or provider does not
/// satisfy the recovery contract.
pub fn restore_data_directory(
    backup: &Path,
    data_directory: &Path,
    storage: &StorageConfiguration,
) -> Result<String, RecoveryError> {
    restore::apply(backup, data_directory, storage).and_then(|report| encode(&report))
}

/// Inspects whether this binary can upgrade one installation without changing it.
///
/// # Errors
///
/// Fails for an unavailable, corrupt, too-old, or newer schema.
pub fn upgrade_preflight(data_directory: &Path) -> Result<String, RecoveryError> {
    upgrade::upgrade(data_directory).and_then(|report| encode(&report))
}

/// Inspects whether this exact binary can be used for a code-only rollback.
///
/// # Errors
///
/// Fails unless the installation schema exactly matches this binary. Schema rollback requires
/// restoring a pre-upgrade backup into a separate empty installation.
pub fn rollback_preflight(data_directory: &Path) -> Result<String, RecoveryError> {
    upgrade::rollback(data_directory).and_then(|report| encode(&report))
}

fn encode(report: &impl Serialize) -> Result<String, RecoveryError> {
    serde_json::to_string_pretty(report).map_err(|_error| RecoveryError::Persistence)
}

#[cfg(test)]
#[path = "recovery_test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
