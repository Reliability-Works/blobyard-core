use super::transfer_validation::{to_i64, validate_provider_upload_id, validate_upload_integrity};
use super::{SqliteRepository, map_error, rows, transfers};
use blobyard_contract::{
    NewUploadPartGrant, ObjectChecksum, RepositoryError, ReservationState, ReservationStrategy,
    UploadPartRecord, UploadReservationRecord,
};
use rusqlite::{Connection, OptionalExtension, Statement, Transaction, params};
use std::collections::HashSet;

const PART_COLUMNS: &str = "p.upload_id, p.part_number, p.expected_size, p.expires_at_ms, p.received_size, p.received_checksum, p.provider_tag";

pub(super) fn attach(
    repository: &SqliteRepository,
    id: &str,
    provider_upload_id: &str,
) -> Result<UploadReservationRecord, RepositoryError> {
    rows::validate_text(id)?;
    validate_provider_upload_id(provider_upload_id)?;
    repository.write_transaction(|transaction| {
        let current = reservation(transaction, id)?;
        require_multipart(&current)?;
        match current.provider_upload_id.as_deref() {
            Some(existing) if existing == provider_upload_id => Ok(current),
            Some(_existing) => Err(RepositoryError::Conflict),
            None => {
                transaction
                    .execute(
                        "UPDATE upload_reservations SET provider_upload_id = ?2 WHERE id = ?1 AND state = 'requested' AND provider_upload_id IS NULL",
                        params![id, provider_upload_id],
                    )
                    .map_err(map_error)?;
                reservation(transaction, id)
            }
        }
    })
}

pub(super) fn issue(
    repository: &SqliteRepository,
    parts: &[NewUploadPartGrant],
) -> Result<Vec<UploadPartRecord>, RepositoryError> {
    validate_batch(parts)?;
    repository.write_transaction(|transaction| {
        let upload = reservation(transaction, &parts[0].upload_id)?;
        let (part_size, part_count) = require_multipart(&upload)?;
        for part in parts {
            let validated = validate_part(part, &upload, part_size, part_count)?;
            upsert_part(transaction, part, validated)?;
        }
        list_on(transaction, &upload.id, Some(parts))
    })
}

pub(super) fn by_capability(
    repository: &SqliteRepository,
    capability_hash: &str,
    now_ms: u64,
) -> Result<(UploadReservationRecord, UploadPartRecord), RepositoryError> {
    ObjectChecksum::new(capability_hash).map_err(|_error| RepositoryError::InvalidInput)?;
    let connection = repository.connection()?;
    let part = connection
        .query_row(
            &format!(
                "SELECT {PART_COLUMNS} FROM upload_parts p JOIN upload_reservations r ON r.id = p.upload_id WHERE p.capability_hash = ?1 AND p.expires_at_ms > ?2 AND r.state = 'requested' AND r.strategy = 'multipart'"
            ),
            params![capability_hash, to_i64(now_ms)?],
            rows::upload_part,
        )
        .map_err(map_error)?;
    let upload = reservation(&connection, &part.upload_id)?;
    drop(connection);
    Ok((upload, part))
}

pub(super) fn record(
    repository: &SqliteRepository,
    upload_id: &str,
    part_number: u32,
    size: u64,
    checksum: &str,
    provider_tag: Option<&str>,
) -> Result<(), RepositoryError> {
    validate_upload_integrity(upload_id, checksum)?;
    if let Some(tag) = provider_tag {
        rows::validate_text(tag)?;
    }
    if part_number == 0 {
        return Err(RepositoryError::InvalidInput);
    }
    let connection = repository.connection()?;
    let expected = connection
        .query_row(
            "SELECT p.expected_size FROM upload_parts p JOIN upload_reservations r ON r.id = p.upload_id WHERE p.upload_id = ?1 AND p.part_number = ?2 AND r.state = 'requested' AND r.strategy = 'multipart'",
            params![upload_id, part_number],
            |row| row.get::<_, i64>(0),
        )
        .map_err(map_error)?;
    if expected != to_i64(size)? {
        return Err(RepositoryError::InvalidInput);
    }
    connection
        .execute(
            "UPDATE upload_parts SET state = 'uploaded', received_size = ?3, received_checksum = ?4, provider_tag = ?5 WHERE upload_id = ?1 AND part_number = ?2",
            params![upload_id, part_number, expected, checksum, provider_tag],
        )
        .map_err(map_error)?;
    drop(connection);
    Ok(())
}

pub(super) fn list(
    repository: &SqliteRepository,
    upload_id: &str,
) -> Result<Vec<UploadPartRecord>, RepositoryError> {
    rows::validate_text(upload_id)?;
    let connection = repository.connection()?;
    reservation(&connection, upload_id)?;
    let result = list_on(&connection, upload_id, None);
    drop(connection);
    result
}

pub(super) fn delete_parts(
    transaction: &Transaction<'_>,
    upload_id: &str,
) -> Result<(), RepositoryError> {
    transaction
        .execute("DELETE FROM upload_parts WHERE upload_id = ?1", [upload_id])
        .map(|_count| ())
        .map_err(map_error)
}

fn validate_batch(parts: &[NewUploadPartGrant]) -> Result<(), RepositoryError> {
    if parts.is_empty() || parts.len() > 100 {
        return Err(RepositoryError::InvalidInput);
    }
    let upload_id = &parts[0].upload_id;
    let mut numbers = HashSet::with_capacity(parts.len());
    if parts
        .iter()
        .any(|part| &part.upload_id != upload_id || !numbers.insert(part.part_number))
    {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_part(
    part: &NewUploadPartGrant,
    upload: &UploadReservationRecord,
    size: u64,
    count: u32,
) -> Result<ValidatedPart, RepositoryError> {
    ObjectChecksum::new(&part.capability_hash).map_err(|_error| RepositoryError::InvalidInput)?;
    let expected_size = to_i64(part.expected_size)?;
    let expires_at_ms = to_i64(part.expires_at_ms)?;
    if part.part_number == 0
        || part.part_number > count
        || part.expected_size != expected_part_size(upload, part.part_number, size)
    {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(ValidatedPart {
        expected_size,
        expires_at_ms,
    })
}

#[derive(Clone, Copy)]
struct ValidatedPart {
    expected_size: i64,
    expires_at_ms: i64,
}

fn expected_part_size(upload: &UploadReservationRecord, number: u32, size: u64) -> u64 {
    let offset = u64::from(number - 1).saturating_mul(size);
    upload.expected_size.saturating_sub(offset).min(size)
}

fn upsert_part(
    transaction: &Transaction<'_>,
    part: &NewUploadPartGrant,
    validated: ValidatedPart,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "INSERT INTO upload_parts (upload_id, part_number, expected_size, capability_hash, expires_at_ms, state) VALUES (?1, ?2, ?3, ?4, ?5, 'pending') ON CONFLICT(upload_id, part_number) DO UPDATE SET capability_hash = excluded.capability_hash, expires_at_ms = excluded.expires_at_ms",
            params![
                part.upload_id,
                part.part_number,
                validated.expected_size,
                part.capability_hash,
                validated.expires_at_ms,
            ],
        )
        .map_err(map_error)
        .map(|_changed| ())
}

fn list_on(
    connection: &Connection,
    upload_id: &str,
    requested: Option<&[NewUploadPartGrant]>,
) -> Result<Vec<UploadPartRecord>, RepositoryError> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {PART_COLUMNS} FROM upload_parts p WHERE p.upload_id = ?1 ORDER BY p.part_number"
        ))
        .map_err(map_error)?;
    let all = query_parts(&mut statement, upload_id)?;
    Ok(match requested {
        Some(parts) => all
            .into_iter()
            .filter(|record| {
                parts
                    .iter()
                    .any(|part| part.part_number == record.part_number)
            })
            .collect(),
        None => all,
    })
}

fn query_parts(
    statement: &mut Statement<'_>,
    upload_id: &str,
) -> Result<Vec<UploadPartRecord>, RepositoryError> {
    statement
        .query_map([upload_id], rows::upload_part)
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)
}

fn reservation(
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
        .optional()
        .map_err(map_error)?
        .ok_or(RepositoryError::NotFound)
}

const fn require_multipart(
    upload: &UploadReservationRecord,
) -> Result<(u64, u32), RepositoryError> {
    match (
        upload.state,
        upload.strategy,
        upload.part_size,
        upload.part_count,
    ) {
        (ReservationState::Requested, ReservationStrategy::Multipart, Some(size), Some(count)) => {
            Ok((size, count))
        }
        _ => Err(RepositoryError::Conflict),
    }
}

#[cfg(test)]
#[path = "transfer_multipart_tests.rs"]
mod tests;
