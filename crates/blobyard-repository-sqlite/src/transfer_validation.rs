use super::{map_error, rows};
use blobyard_contract::{
    NewUploadReservation, ObjectChecksum, RepositoryError, ReservationStrategy, StorageKey,
};
use rusqlite::{Connection, params};

const MAX_PROVIDER_UPLOAD_ID_LENGTH: usize = 4_096;

pub(super) struct ValidatedReservation {
    pub(super) expected_size: i64,
    pub(super) expires_at_ms: i64,
    pub(super) created_at_ms: i64,
    pub(super) part_size: Option<i64>,
}

pub(super) fn validate_reservation(
    value: &NewUploadReservation,
) -> Result<ValidatedReservation, RepositoryError> {
    for field in [
        &value.id,
        &value.project_id,
        &value.object_path,
        &value.filename,
        &value.content_type,
    ] {
        rows::validate_text(field)?;
    }
    ObjectChecksum::new(&value.expected_checksum)
        .map_err(|_error| RepositoryError::InvalidInput)?;
    ObjectChecksum::new(&value.capability_hash).map_err(|_error| RepositoryError::InvalidInput)?;
    StorageKey::new(value.storage_key.clone()).map_err(|_error| RepositoryError::InvalidInput)?;
    validate_provenance(
        value.git_repository.as_deref(),
        value.git_commit.as_deref(),
        value.git_branch.as_deref(),
    )?;
    validate_strategy(value)?;
    let created_at_ms = to_i64(value.created_at_ms)?;
    let expires_at_ms = to_i64(value.expires_at_ms)?;
    if expires_at_ms <= created_at_ms {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(ValidatedReservation {
        expected_size: to_i64(value.expected_size)?,
        expires_at_ms,
        created_at_ms,
        part_size: value.part_size.map(to_i64).transpose()?,
    })
}

fn validate_strategy(value: &NewUploadReservation) -> Result<(), RepositoryError> {
    match (value.strategy, value.part_size, value.part_count) {
        (ReservationStrategy::Single, None, None) => Ok(()),
        (ReservationStrategy::Multipart, Some(size), Some(count))
            if size > 0 && count > 0 && count <= 10_000 =>
        {
            let expected = value.expected_size.div_ceil(size);
            if expected == u64::from(count) {
                Ok(())
            } else {
                Err(RepositoryError::InvalidInput)
            }
        }
        _ => Err(RepositoryError::InvalidInput),
    }
}

pub(super) fn validate_provenance(
    repository: Option<&str>,
    commit: Option<&str>,
    branch: Option<&str>,
) -> Result<(), RepositoryError> {
    for value in [repository, commit, branch].into_iter().flatten() {
        rows::validate_text(value)?;
    }
    Ok(())
}

pub(super) fn to_i64(value: u64) -> Result<i64, RepositoryError> {
    i64::try_from(value).map_err(|_error| RepositoryError::InvalidInput)
}

pub(super) fn validate_upload_integrity(id: &str, checksum: &str) -> Result<(), RepositoryError> {
    rows::validate_text(id)?;
    ObjectChecksum::new(checksum)
        .map(|_checksum| ())
        .map_err(|_error| RepositoryError::InvalidInput)
}

pub(super) fn validate_provider_upload_id(value: &str) -> Result<(), RepositoryError> {
    if value.is_empty()
        || value.len() > MAX_PROVIDER_UPLOAD_ID_LENGTH
        || value.chars().any(char::is_control)
    {
        Err(RepositoryError::InvalidInput)
    } else {
        Ok(())
    }
}

pub(super) fn renew_requested(
    connection: &Connection,
    id: &str,
    expires_at_ms: u64,
) -> Result<(), RepositoryError> {
    rows::validate_text(id)?;
    let changed = connection
        .execute(
            "UPDATE upload_reservations SET expires_at_ms = ?2 WHERE id = ?1 AND state = 'requested'",
            params![id, to_i64(expires_at_ms)?],
        )
        .map_err(map_error)?;
    if changed == 1 {
        Ok(())
    } else {
        Err(RepositoryError::Conflict)
    }
}
