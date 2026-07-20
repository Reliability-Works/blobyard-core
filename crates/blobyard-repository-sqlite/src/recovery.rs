use blobyard_contract::RepositoryError;
use rusqlite::{Connection, MAIN_DB, OpenFlags};
use std::path::Path;

/// Read-only facts used by standalone backup and upgrade commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DatabaseInspection {
    /// Applied `SQLite` schema version.
    pub schema_version: u32,
}

/// Returns the newest schema understood by this binary.
#[must_use]
pub const fn current_schema_version() -> u32 {
    super::adapter::SqliteRepository::supported_schema_version()
}

/// Returns the oldest populated schema this binary can upgrade directly.
#[must_use]
pub const fn oldest_supported_schema_version() -> u32 {
    1
}

/// Checks one existing metadata database without applying migrations.
///
/// # Errors
///
/// Returns a stable repository failure when the database is missing, unreadable, or corrupt.
pub fn inspect_database(path: &Path) -> Result<DatabaseInspection, RepositoryError> {
    let connection = open_read_only(path)?;
    inspect_connection(&connection)
}

/// Creates a consistent online `SQLite` snapshot without changing the source database.
///
/// # Errors
///
/// Returns a stable repository failure when the source is unreadable, corrupt, newer than this
/// binary, or cannot be copied and verified at the destination.
pub fn snapshot_database(
    source: &Path,
    destination: &Path,
) -> Result<DatabaseInspection, RepositoryError> {
    let connection = open_read_only(source)?;
    let inspection = inspect_connection(&connection)?;
    if inspection.schema_version > current_schema_version() {
        return Err(RepositoryError::SchemaTooNew);
    }
    connection
        .backup(MAIN_DB, destination, None)
        .map_err(|_error| RepositoryError::Unavailable)?;
    validate_copied_snapshot(destination, inspection)
}

fn validate_copied_snapshot(
    destination: &Path,
    source: DatabaseInspection,
) -> Result<DatabaseInspection, RepositoryError> {
    let copied = inspect_database(destination)?;
    validate_snapshot(source, copied)
}

fn validate_snapshot(
    source: DatabaseInspection,
    copied: DatabaseInspection,
) -> Result<DatabaseInspection, RepositoryError> {
    if copied == source {
        Ok(copied)
    } else {
        Err(RepositoryError::Unavailable)
    }
}

fn open_read_only(path: &Path) -> Result<Connection, RepositoryError> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_error| RepositoryError::Unavailable)
}

fn inspect_connection(connection: &Connection) -> Result<DatabaseInspection, RepositoryError> {
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|_error| RepositoryError::Unavailable)?;
    finish_inspection(
        &integrity,
        super::adapter::SqliteRepository::read_schema_version(connection),
    )
}

fn finish_inspection(
    integrity: &str,
    schema_version: Result<u32, RepositoryError>,
) -> Result<DatabaseInspection, RepositoryError> {
    validate_integrity(integrity)?;
    let schema_version = schema_version?;
    Ok(DatabaseInspection { schema_version })
}

fn validate_integrity(value: &str) -> Result<(), RepositoryError> {
    if value == "ok" {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
