use super::transfer_validation::to_i64;
use super::{SqliteRepository, map_error, rows};
use blobyard_contract::{NewDownloadGrant, ObjectChecksum, RepositoryError, StoredObjectRecord};
use rusqlite::params;

pub(super) fn issue(
    repository: &SqliteRepository,
    grant: &NewDownloadGrant,
) -> Result<(), RepositoryError> {
    let expires_at_ms = validate(grant)?;
    let changed = repository
        .connection()?
        .execute(
            "INSERT INTO download_grants (capability_hash, version_id, expires_at_ms) SELECT ?1, id, ?3 FROM object_versions WHERE id = ?2 AND state = 'complete'",
            params![grant.capability_hash, grant.version_id, expires_at_ms],
        )
        .map_err(map_error)?;
    if changed == 1 {
        Ok(())
    } else {
        Err(RepositoryError::NotFound)
    }
}

pub(super) fn resolve(
    repository: &SqliteRepository,
    capability_hash: &str,
    now_ms: u64,
) -> Result<StoredObjectRecord, RepositoryError> {
    ObjectChecksum::new(capability_hash).map_err(|_error| RepositoryError::InvalidInput)?;
    repository
        .connection()?
        .query_row(
            &format!("SELECT {} FROM download_grants d JOIN object_versions v ON v.id = d.version_id JOIN upload_reservations r ON r.version_id = v.id WHERE d.capability_hash = ?1 AND d.expires_at_ms > ?2 AND v.state = 'complete'", rows::STORED_COLUMNS),
            params![capability_hash, to_i64(now_ms)?],
            rows::stored_object,
        )
        .map_err(map_error)
}

fn validate(value: &NewDownloadGrant) -> Result<i64, RepositoryError> {
    rows::validate_text(&value.version_id)?;
    ObjectChecksum::new(&value.capability_hash).map_err(|_error| RepositoryError::InvalidInput)?;
    to_i64(value.expires_at_ms)
}
