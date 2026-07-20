#[path = "auth.rs"]
mod auth;
#[path = "auth_machine.rs"]
mod auth_machine;
#[path = "auth_validation.rs"]
mod auth_validation;
#[path = "ci.rs"]
mod ci;
#[path = "ci_match.rs"]
mod ci_match;
#[path = "ci_records.rs"]
mod ci_records;
#[path = "ci_validation.rs"]
mod ci_validation;
#[path = "inbox_queries.rs"]
mod inbox_queries;
#[path = "inbox_rates.rs"]
mod inbox_rates;
#[path = "inbox_uploads.rs"]
mod inbox_uploads;
#[path = "inboxes.rs"]
mod inboxes;
#[path = "inventory.rs"]
mod inventory;
#[path = "lifecycle.rs"]
mod lifecycle;
#[path = "lifecycle_audit.rs"]
mod lifecycle_audit;
#[path = "lifecycle_deletion.rs"]
mod lifecycle_deletion;
#[path = "lifecycle_operation.rs"]
mod lifecycle_operation;
#[path = "lifecycle_retention.rs"]
mod lifecycle_retention;
#[path = "lifecycle_retention_plan.rs"]
mod lifecycle_retention_plan;
#[path = "metadata.rs"]
mod metadata;
#[path = "migration.rs"]
mod migration;
#[path = "migrations.rs"]
mod migrations;
#[path = "preview_queries.rs"]
mod preview_queries;
#[path = "previews.rs"]
mod previews;
#[path = "rows.rs"]
mod rows;
#[path = "sharing.rs"]
mod sharing;
#[path = "sharing_queries.rs"]
mod sharing_queries;
#[path = "transfer_downloads.rs"]
mod transfer_downloads;
#[path = "transfer_listing.rs"]
mod transfer_listing;
#[path = "transfer_multipart.rs"]
mod transfer_multipart;
#[path = "transfer_reservations.rs"]
mod transfer_reservations;
#[path = "transfer_validation.rs"]
mod transfer_validation;
#[path = "transfers.rs"]
mod transfers;
#[path = "yard_cleanup.rs"]
mod yard_cleanup;
#[path = "yard_finalise.rs"]
mod yard_finalise;
#[path = "yard_history.rs"]
mod yard_history;
#[path = "yard_lifecycle.rs"]
mod yard_lifecycle;
#[path = "yard_queries.rs"]
mod yard_queries;
#[path = "yard_rows.rs"]
mod yard_rows;
#[path = "yard_start.rs"]
mod yard_start;
#[path = "yard_validation.rs"]
mod yard_validation;
#[path = "yards.rs"]
mod yards;

use blobyard_contract::{NewAuditEvent, RepositoryError};
use rusqlite::{Connection, ErrorCode as SqliteErrorCode, Transaction};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// `SQLite` metadata repository configured for foreign keys and WAL durability.
#[derive(Debug)]
pub struct SqliteRepository {
    connection: Mutex<Connection>,
}

impl SqliteRepository {
    /// Opens or creates a repository and applies supported forward-only migrations.
    ///
    /// # Errors
    ///
    /// Returns a stable repository error when `SQLite` cannot open, configure, or migrate the file.
    pub fn open(path: &Path) -> Result<Self, RepositoryError> {
        let connection = Connection::open(path).map_err(map_error)?;
        Self::initialize_connection(connection)
    }

    fn initialize_connection(mut connection: Connection) -> Result<Self, RepositoryError> {
        configure(&connection)?;
        migrations::apply(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, RepositoryError> {
        self.connection
            .lock()
            .map_err(|_error| RepositoryError::Unavailable)
    }

    /// Returns the raw connection lock for isolated fault-injection tests.
    #[cfg(any(test, feature = "test-seams"))]
    #[doc(hidden)]
    pub fn test_connection(&self) -> std::sync::LockResult<MutexGuard<'_, Connection>> {
        self.connection.lock()
    }

    /// Executes the cleanup header decoder for isolated integration fault injection.
    #[cfg(feature = "test-seams")]
    #[doc(hidden)]
    pub fn test_yard_cleanup_query(
        statement: &mut rusqlite::Statement<'_>,
        yard_id: Option<&str>,
    ) -> Result<Vec<(String, String, String, String)>, RepositoryError> {
        yard_cleanup::query_headers(statement, yard_id)
    }

    fn write_transaction<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, RepositoryError>,
    ) -> Result<T, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(map_error)?;
        let result = operation(&transaction)?;
        transaction.commit().map_err(map_error)?;
        drop(connection);
        Ok(result)
    }

    pub(crate) const fn supported_schema_version() -> u32 {
        migrations::CURRENT_SCHEMA_VERSION
    }

    pub(crate) fn read_schema_version(connection: &Connection) -> Result<u32, RepositoryError> {
        migrations::schema_version(connection)
    }
}

fn configure(connection: &Connection) -> Result<(), RepositoryError> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
        )
        .map_err(map_error)
}

fn validate_record(id: &str, name: &str) -> Result<(), RepositoryError> {
    rows::validate_text(id)?;
    rows::validate_text(name)
}

fn collect<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>, RepositoryError> {
    rows.collect::<Result<Vec<_>, _>>().map_err(map_error)
}

const fn changed_once(changed: usize) -> Result<(), RepositoryError> {
    if changed == 1 {
        Ok(())
    } else {
        Err(RepositoryError::Conflict)
    }
}

fn finish_audited_change(
    transaction: &Transaction<'_>,
    changed: usize,
    event: &NewAuditEvent,
) -> Result<bool, RepositoryError> {
    changed_once(changed)?;
    lifecycle_audit::insert(transaction, event)?;
    Ok(true)
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
fn repository_with_transfers() -> (tempfile::TempDir, SqliteRepository) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");
    blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
    blobyard_testkit::transfer_conformance(&repository, "project_fixture")
        .expect("transfer conformance");
    (temporary, repository)
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
fn execute_corruption(repository: &SqliteRepository, mutation: &str) {
    repository
        .test_connection()
        .expect("connection")
        .execute_batch(&format!("PRAGMA ignore_check_constraints = ON; {mutation}"))
        .expect("corruption statement");
}

#[cfg(test)]
trait CapabilityCorruptionFixture {
    type ListedRecord;
    type ResolvedRecord;

    fn repository(&self) -> &SqliteRepository;
    fn list(&self) -> Result<Vec<Self::ListedRecord>, RepositoryError>;
    fn resolve(&self) -> Result<Self::ResolvedRecord, RepositoryError>;
}

#[cfg(test)]
fn assert_capability_corruption(
    fixture: &impl CapabilityCorruptionFixture,
    mutation: &str,
    resolved_error: RepositoryError,
) {
    execute_corruption(fixture.repository(), mutation);
    assert_eq!(fixture.list().err(), Some(RepositoryError::Unavailable));
    assert_eq!(fixture.resolve().err(), Some(resolved_error));
}

fn map_error(error: rusqlite::Error) -> RepositoryError {
    match error {
        rusqlite::Error::QueryReturnedNoRows => RepositoryError::NotFound,
        rusqlite::Error::SqliteFailure(failure, _message)
            if matches!(
                failure.code,
                SqliteErrorCode::ConstraintViolation | SqliteErrorCode::TypeMismatch
            ) =>
        {
            RepositoryError::Conflict
        }
        _ => RepositoryError::Unavailable,
    }
}

#[cfg(test)]
#[path = "adapter_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "adapter_query_tests.rs"]
mod query_tests;

#[cfg(test)]
#[path = "ci_test_fixtures.rs"]
mod ci_test_fixtures;
