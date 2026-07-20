use super::{RecoveryError, io, map_repository};
use blobyard_repository_sqlite as repository;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UpgradeReport {
    report_schema_version: u32,
    operation: &'static str,
    core_version: &'static str,
    source_schema_version: u32,
    target_schema_version: u32,
    oldest_supported_schema_version: u32,
    migration_required: bool,
    backup_required: bool,
    rollback_constraint: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RollbackReport {
    report_schema_version: u32,
    operation: &'static str,
    core_version: &'static str,
    metadata_schema_version: u32,
    binary_schema_version: u32,
    code_only_rollback_allowed: bool,
}

pub(super) fn upgrade(data_directory: &Path) -> Result<UpgradeReport, RecoveryError> {
    let schema = inspect(data_directory)?;
    let oldest = repository::oldest_supported_schema_version();
    let target = repository::current_schema_version();
    if schema < oldest {
        return Err(RecoveryError::SchemaTooOld);
    }
    if schema > target {
        return Err(RecoveryError::SchemaTooNew);
    }
    Ok(UpgradeReport {
        report_schema_version: 1,
        operation: "upgrade-preflight",
        core_version: env!("CARGO_PKG_VERSION"),
        source_schema_version: schema,
        target_schema_version: target,
        oldest_supported_schema_version: oldest,
        migration_required: schema != target,
        backup_required: true,
        rollback_constraint: "schema changes require restoring a pre-upgrade backup into an empty installation",
    })
}

pub(super) fn rollback(data_directory: &Path) -> Result<RollbackReport, RecoveryError> {
    let schema = inspect(data_directory)?;
    let binary = repository::current_schema_version();
    if schema != binary {
        return Err(RecoveryError::RollbackUnsafe);
    }
    Ok(RollbackReport {
        report_schema_version: 1,
        operation: "rollback-preflight",
        core_version: env!("CARGO_PKG_VERSION"),
        metadata_schema_version: schema,
        binary_schema_version: binary,
        code_only_rollback_allowed: true,
    })
}

fn inspect(data_directory: &Path) -> Result<u32, RecoveryError> {
    io::validate_directory(data_directory)?;
    let runtime = io::read_secure_file(data_directory, Path::new("runtime.secret"))
        .map_err(|_error| RecoveryError::InstallationUnavailable)?;
    let runtime =
        std::str::from_utf8(&runtime).map_err(|_error| RecoveryError::InstallationUnavailable)?;
    blobyard_core::SecretString::new(runtime.to_owned())
        .map_err(|_error| RecoveryError::InstallationUnavailable)?;
    repository::inspect_database(&data_directory.join("metadata.sqlite3"))
        .map(|inspection| inspection.schema_version)
        .map_err(map_repository)
}

#[cfg(test)]
#[path = "recovery_upgrade_tests.rs"]
mod tests;
