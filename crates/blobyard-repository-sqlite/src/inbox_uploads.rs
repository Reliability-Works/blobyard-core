use super::{
    SqliteRepository, changed_once, inbox_queries, lifecycle_audit, map_error, transfer_multipart,
    transfer_reservations, transfer_validation, transfers,
};
use blobyard_contract::{
    InboxStatus, NewAuditEvent, NewInboxUpload, NewUploadReservation, ObjectChecksum, ObjectSource,
    ObjectVersionRecord, RepositoryError, ReservationState, UploadReservationRecord,
};
use rusqlite::{Transaction, params};

pub(super) fn reserve(
    repository: &SqliteRepository,
    inbox_upload: &NewInboxUpload,
    reservation: &NewUploadReservation,
) -> Result<UploadReservationRecord, RepositoryError> {
    let validated = transfer_validation::validate_reservation(reservation)?;
    inbox_queries::validate_capability(&inbox_upload.capability_hash)?;
    ObjectChecksum::new(&inbox_upload.fingerprint_hash)
        .map_err(|_error| RepositoryError::InvalidInput)?;
    let now = transfer_validation::to_i64(inbox_upload.now_ms)?;
    repository.write_transaction(|transaction| {
        let inbox = inbox_queries::active_by_capability(
            transaction,
            &inbox_upload.capability_hash,
            now,
        )?;
        validate_scope(&inbox, reservation, inbox_upload.now_ms)?;
        reserve_capacity(transaction, &inbox.id, reservation.expected_size)?;
        transfer_reservations::ensure_project(transaction, &reservation.project_id)?;
        let version = transfer_reservations::next_version(transaction, reservation)?;
        transfer_reservations::insert_version(
            transaction,
            reservation,
            version,
            validated.created_at_ms,
        )?;
        transfer_reservations::insert_reservation(transaction, reservation, &validated)?;
        transaction
            .execute(
                "INSERT INTO inbox_uploads (upload_id, inbox_id, fingerprint_hash, reserved_size, status, created_at_ms) VALUES (?1, ?2, ?3, ?4, 'reserved', ?5)",
                params![
                    reservation.id,
                    inbox.id,
                    inbox_upload.fingerprint_hash,
                    validated.expected_size,
                    validated.created_at_ms,
                ],
            )
            .map_err(map_error)?;
        transfer_reservations::by_id(transaction, &reservation.id)
    })
}

pub(super) fn complete(
    repository: &SqliteRepository,
    capability_hash: &str,
    upload_id: &str,
    now_ms: u64,
    event: &NewAuditEvent,
) -> Result<ObjectVersionRecord, RepositoryError> {
    validate_access(capability_hash, upload_id)?;
    let now = transfer_validation::to_i64(now_ms)?;
    repository.write_transaction(|transaction| {
        let reservation = inbox_reservation(transaction, capability_hash, upload_id, now)?;
        transfers::require_state(reservation.state, ReservationState::Uploaded)?;
        let inbox_id = reserved_inbox_id(transaction, upload_id)?;
        validate_event(
            event,
            "inbox.uploaded",
            &inbox_id,
            reservation.expected_size,
            now_ms,
        )?;
        transfers::update_reservation_state(transaction, upload_id, "uploaded", "complete", false)?;
        complete_version(transaction, &reservation)?;
        complete_capacity(transaction, &inbox_id, reservation.expected_size)?;
        transition_link(transaction, upload_id, "complete")?;
        lifecycle_audit::insert(transaction, event)?;
        transfers::object_version_by_id(transaction, &reservation.version.id)
    })
}

pub(super) fn abort(
    repository: &SqliteRepository,
    capability_hash: &str,
    upload_id: &str,
    now_ms: u64,
) -> Result<UploadReservationRecord, RepositoryError> {
    validate_access(capability_hash, upload_id)?;
    let now = transfer_validation::to_i64(now_ms)?;
    repository.write_transaction(|transaction| {
        let reservation = inbox_reservation(transaction, capability_hash, upload_id, now)?;
        if !matches!(
            reservation.state,
            ReservationState::Requested | ReservationState::Uploaded
        ) {
            return Err(RepositoryError::Conflict);
        }
        let inbox_id = reserved_inbox_id(transaction, upload_id)?;
        transfers::update_reservation_state(
            transaction,
            upload_id,
            reservation.state.as_str(),
            "aborted",
            true,
        )?;
        transfer_multipart::delete_parts(transaction, upload_id)?;
        abort_version(transaction, &reservation.version.id)?;
        release_capacity(transaction, &inbox_id, reservation.expected_size)?;
        transition_link(transaction, upload_id, "aborted")?;
        Ok(reservation)
    })
}

fn inbox_reservation(
    connection: &rusqlite::Connection,
    capability_hash: &str,
    upload_id: &str,
    now: i64,
) -> Result<UploadReservationRecord, RepositoryError> {
    let reservation = inbox_queries::upload(connection, capability_hash, upload_id, now)?;
    require_inbox_source(&reservation)?;
    Ok(reservation)
}

fn validate_scope(
    inbox: &blobyard_contract::InboxRecord,
    reservation: &NewUploadReservation,
    now_ms: u64,
) -> Result<(), RepositoryError> {
    let valid = inbox.status == InboxStatus::Active
        && inbox.project_id == reservation.project_id
        && reservation.source == ObjectSource::Inbox
        && reservation.created_at_ms == now_ms
        && reservation.expires_at_ms <= inbox.expires_at_ms;
    if valid {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

fn reserve_capacity(
    transaction: &Transaction<'_>,
    inbox_id: &str,
    size: u64,
) -> Result<(), RepositoryError> {
    let size = transfer_validation::to_i64(size)?;
    let changed = transaction
        .execute(
            "UPDATE inboxes SET reserved_files = reserved_files + 1, reserved_bytes = reserved_bytes + ?2 WHERE id = ?1 AND status = 'active' AND current_files + reserved_files < maximum_files AND current_bytes + reserved_bytes + ?2 <= maximum_bytes",
            params![inbox_id, size],
        )
        .map_err(map_error)?;
    if changed == 1 {
        Ok(())
    } else {
        Err(RepositoryError::Conflict)
    }
}

fn complete_version(
    transaction: &Transaction<'_>,
    reservation: &UploadReservationRecord,
) -> Result<(), RepositoryError> {
    let changed = transaction
        .execute(
            "UPDATE object_versions SET state = 'complete', size = ?2, checksum = ?3 WHERE id = ?1 AND state = 'pending'",
            params![
                reservation.version.id,
                transfer_validation::to_i64(reservation.expected_size)?,
                reservation.expected_checksum,
            ],
        )
        .map_err(map_error)?;
    changed_once(changed)
}

fn abort_version(transaction: &Transaction<'_>, version_id: &str) -> Result<(), RepositoryError> {
    let changed = transaction
        .execute(
            "UPDATE object_versions SET state = 'aborted' WHERE id = ?1 AND state = 'pending'",
            [version_id],
        )
        .map_err(map_error)?;
    changed_once(changed)
}

fn reserved_inbox_id(
    transaction: &Transaction<'_>,
    upload_id: &str,
) -> Result<String, RepositoryError> {
    transaction
        .query_row(
            "SELECT inbox_id FROM inbox_uploads WHERE upload_id = ?1 AND status = 'reserved'",
            [upload_id],
            |row| row.get(0),
        )
        .map_err(map_error)
}

fn complete_capacity(
    transaction: &Transaction<'_>,
    inbox_id: &str,
    size: u64,
) -> Result<(), RepositoryError> {
    let size = transfer_validation::to_i64(size)?;
    let changed = transaction
        .execute(
            "UPDATE inboxes SET reserved_files = reserved_files - 1, reserved_bytes = reserved_bytes - ?2, current_files = current_files + 1, current_bytes = current_bytes + ?2 WHERE id = ?1 AND reserved_files >= 1 AND reserved_bytes >= ?2",
            params![inbox_id, size],
        )
        .map_err(map_error)?;
    changed_once(changed)
}

fn release_capacity(
    transaction: &Transaction<'_>,
    inbox_id: &str,
    size: u64,
) -> Result<(), RepositoryError> {
    let size = transfer_validation::to_i64(size)?;
    let changed = transaction
        .execute(
            "UPDATE inboxes SET reserved_files = reserved_files - 1, reserved_bytes = reserved_bytes - ?2 WHERE id = ?1 AND reserved_files >= 1 AND reserved_bytes >= ?2",
            params![inbox_id, size],
        )
        .map_err(map_error)?;
    changed_once(changed)
}

fn transition_link(
    transaction: &Transaction<'_>,
    upload_id: &str,
    status: &str,
) -> Result<(), RepositoryError> {
    let changed = transaction
        .execute(
            "UPDATE inbox_uploads SET status = ?2 WHERE upload_id = ?1 AND status = 'reserved'",
            params![upload_id, status],
        )
        .map_err(map_error)?;
    changed_once(changed)
}

fn validate_access(capability_hash: &str, upload_id: &str) -> Result<(), RepositoryError> {
    inbox_queries::validate_capability(capability_hash)?;
    super::rows::validate_text(upload_id)
}

fn require_inbox_source(reservation: &UploadReservationRecord) -> Result<(), RepositoryError> {
    if reservation.version.source == ObjectSource::Inbox {
        Ok(())
    } else {
        Err(RepositoryError::NotFound)
    }
}

fn validate_event(
    event: &NewAuditEvent,
    action: &str,
    inbox_id: &str,
    byte_size: u64,
    created_at_ms: u64,
) -> Result<(), RepositoryError> {
    let valid = event.action == action
        && event.target_type == "object_version"
        && event.actor == inbox_id
        && event.created_at_ms == created_at_ms
        && event.metadata
            == [
                (
                    "byteSize".to_owned(),
                    blobyard_contract::AuditValue::Number(byte_size),
                ),
                (
                    "source".to_owned(),
                    blobyard_contract::AuditValue::String("inbox".to_owned()),
                ),
            ];
    if valid {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

#[cfg(test)]
#[path = "inbox_uploads_tests.rs"]
mod tests;
