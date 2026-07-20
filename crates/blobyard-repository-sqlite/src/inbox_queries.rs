use super::{map_error, rows, transfer_reservations};
use blobyard_contract::{InboxRecord, InboxStatus, ObjectChecksum, RepositoryError};
use rusqlite::{Connection, OptionalExtension, Row, Statement, params};

pub(super) const COLUMNS: &str = "id, workspace_id, project_id, name, expires_at_ms, status, current_files, current_bytes, reserved_files, reserved_bytes, maximum_files, maximum_bytes, created_at_ms, revoked_at_ms";

pub(super) fn validate_capability(value: &str) -> Result<(), RepositoryError> {
    ObjectChecksum::new(value)
        .map(|_checksum| ())
        .map_err(|_error| RepositoryError::InvalidInput)
}

pub(super) fn validate_name(value: &str) -> Result<(), RepositoryError> {
    rows::validate_text(value)?;
    if value.len() <= 128 && !value.chars().any(char::is_control) {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

pub(super) fn by_id(
    connection: &Connection,
    inbox_id: &str,
) -> Result<InboxRecord, RepositoryError> {
    connection
        .query_row(
            &format!("SELECT {COLUMNS} FROM inboxes WHERE id = ?1"),
            [inbox_id],
            row,
        )
        .map_err(map_error)
}

pub(super) fn active_by_capability(
    connection: &Connection,
    capability_hash: &str,
    now_ms: i64,
) -> Result<InboxRecord, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {COLUMNS} FROM inboxes WHERE capability_hash = ?1 AND status = 'active' AND expires_at_ms > ?2"
            ),
            params![capability_hash, now_ms],
            row,
        )
        .map_err(map_error)
}

pub(super) fn list(
    statement: &mut Statement<'_>,
    project_id: &str,
) -> Result<Vec<InboxRecord>, RepositoryError> {
    statement
        .query_map([project_id], row)
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)
}

pub(super) fn upload(
    connection: &Connection,
    capability_hash: &str,
    upload_id: &str,
    now_ms: i64,
) -> Result<blobyard_contract::UploadReservationRecord, RepositoryError> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM inbox_uploads iu JOIN inboxes i ON i.id = iu.inbox_id WHERE iu.upload_id = ?1 AND i.capability_hash = ?2 AND i.status = 'active' AND i.expires_at_ms > ?3 LIMIT 1",
            params![upload_id, capability_hash, now_ms],
            |_row| Ok(()),
        )
        .optional()
        .map_err(map_error)?;
    if exists.is_none() {
        return Err(RepositoryError::NotFound);
    }
    transfer_reservations::by_id(connection, upload_id)
}

fn row(row: &Row<'_>) -> rusqlite::Result<InboxRecord> {
    let status: String = row.get(5)?;
    Ok(InboxRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        project_id: row.get(2)?,
        name: row.get(3)?,
        expires_at_ms: required_u64(row.get(4)?)?,
        status: InboxStatus::parse(&status).ok_or_else(|| conversion_error(status))?,
        current_files: required_u64(row.get(6)?)?,
        current_bytes: required_u64(row.get(7)?)?,
        reserved_files: required_u64(row.get(8)?)?,
        reserved_bytes: required_u64(row.get(9)?)?,
        maximum_files: required_u64(row.get(10)?)?,
        maximum_bytes: required_u64(row.get(11)?)?,
        created_at_ms: required_u64(row.get(12)?)?,
        revoked_at_ms: optional_u64(row.get(13)?)?,
    })
}

fn required_u64(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(conversion_error)
}

fn optional_u64(value: Option<i64>) -> rusqlite::Result<Option<u64>> {
    value.map(required_u64).transpose()
}

fn conversion_error(value: impl std::fmt::Debug + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Integer,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{value:?}"),
        )),
    )
}

#[cfg(test)]
#[path = "inbox_queries_tests.rs"]
mod tests;
