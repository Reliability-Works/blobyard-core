use super::transfer_validation::{ValidatedReservation, validate_reservation};
use super::{SqliteRepository, map_error, rows, transfers};
use blobyard_contract::{NewUploadReservation, RepositoryError, UploadReservationRecord};
use rusqlite::{Connection, Transaction, params};

pub(super) fn reserve(
    repository: &SqliteRepository,
    reservation: &NewUploadReservation,
) -> Result<UploadReservationRecord, RepositoryError> {
    let validated = validate_reservation(reservation)?;
    let mut connection = repository.connection()?;
    let transaction = connection.transaction().map_err(map_error)?;
    ensure_project(&transaction, &reservation.project_id)?;
    let version = next_version(&transaction, reservation)?;
    insert_version(&transaction, reservation, version, validated.created_at_ms)?;
    insert_reservation(&transaction, reservation, &validated)?;
    let result = by_id(&transaction, &reservation.id)?;
    transaction.commit().map_err(map_error)?;
    drop(connection);
    Ok(result)
}

pub(super) fn by_id(
    connection: &Connection,
    id: &str,
) -> Result<UploadReservationRecord, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM upload_reservations r JOIN object_versions v ON v.id = r.version_id WHERE r.id = ?1",
                transfers::RESERVATION_COLUMNS
            ),
            [id],
            rows::upload_reservation,
        )
        .map_err(map_error)
}

pub(super) fn ensure_project(
    transaction: &Transaction<'_>,
    id: &str,
) -> Result<(), RepositoryError> {
    let exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)",
            [id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    if exists {
        Ok(())
    } else {
        Err(RepositoryError::NotFound)
    }
}

pub(super) fn next_version(
    transaction: &Transaction<'_>,
    value: &NewUploadReservation,
) -> Result<i64, RepositoryError> {
    let version: i64 = transaction
        .query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM (SELECT version FROM object_versions WHERE project_id = ?1 AND object_path = ?2 UNION ALL SELECT item.version FROM deletion_items item JOIN deletion_operations operation ON operation.id = item.operation_id WHERE operation.project_id = ?1 AND operation.object_path = ?2)",
            params![value.project_id, value.object_path],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    u64::try_from(version).map_err(|_error| RepositoryError::Unavailable)?;
    Ok(version)
}

pub(super) fn insert_version(
    transaction: &Transaction<'_>,
    value: &NewUploadReservation,
    version: i64,
    created_at_ms: i64,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "INSERT INTO object_versions (id, project_id, object_path, version, storage_key, state, created_at_ms, source, git_repository, git_commit, git_branch) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8, ?9, ?10)",
            params![
                value.id,
                value.project_id,
                value.object_path,
                version,
                value.storage_key,
                created_at_ms,
                value.source.as_str(),
                value.git_repository,
                value.git_commit,
                value.git_branch,
            ],
        )
        .map(|_count| ())
        .map_err(map_error)
}

pub(super) fn insert_reservation(
    transaction: &Transaction<'_>,
    value: &NewUploadReservation,
    validated: &ValidatedReservation,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "INSERT INTO upload_reservations (id, version_id, filename, content_type, expected_size, expected_checksum, capability_hash, expires_at_ms, state, strategy, part_size, part_count) VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, 'requested', ?8, ?9, ?10)",
            params![
                value.id,
                value.filename,
                value.content_type,
                validated.expected_size,
                value.expected_checksum,
                value.capability_hash,
                validated.expires_at_ms,
                value.strategy.as_str(),
                validated.part_size,
                value.part_count,
            ],
        )
        .map(|_count| ())
        .map_err(map_error)
}
