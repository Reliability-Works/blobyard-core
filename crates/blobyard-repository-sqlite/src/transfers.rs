use super::transfer_downloads;
use super::transfer_listing;
use super::transfer_multipart;
use super::transfer_reservations::{by_id as reservation_by_id, reserve};
use super::transfer_validation::{renew_requested, to_i64, validate_upload_integrity};
use super::{SqliteRepository, changed_once, map_error, rows};
use blobyard_contract::{
    NewDownloadGrant, NewUploadReservation, ObjectChecksum, ObjectVersionRecord, RepositoryError,
    ReservationState, StoredObjectRecord, TransferRepository, UploadReservationRecord,
};
use rusqlite::{Connection, Transaction, params};

pub(super) const RESERVATION_COLUMNS: &str = "r.id, v.id, v.project_id, v.object_path, v.version, v.storage_key, v.state, v.size, v.checksum, v.created_at_ms, v.source, v.git_repository, v.git_commit, v.git_branch, r.filename, r.content_type, r.expected_size, r.expected_checksum, r.expires_at_ms, r.state, r.strategy, r.part_size, r.part_count, r.provider_upload_id";

impl TransferRepository for SqliteRepository {
    fn reserve_upload(
        &self,
        reservation: &NewUploadReservation,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        reserve(self, reservation)
    }

    fn upload_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        ObjectChecksum::new(capability_hash).map_err(|_error| RepositoryError::InvalidInput)?;
        let now_ms = to_i64(now_ms)?;
        let connection = self.connection()?;
        connection
            .query_row(
                &format!(
                    "SELECT {RESERVATION_COLUMNS} FROM upload_reservations r JOIN object_versions v ON v.id = r.version_id WHERE r.capability_hash = ?1 AND r.state = 'requested' AND r.expires_at_ms > ?2"
                ),
                params![capability_hash, now_ms],
                rows::upload_reservation,
            )
            .map_err(map_error)
    }

    fn upload_by_id(&self, id: &str) -> Result<UploadReservationRecord, RepositoryError> {
        rows::validate_text(id)?;
        let connection = self.connection()?;
        reservation_by_id(&connection, id)
    }

    fn renew_upload(&self, id: &str, expires_at_ms: u64) -> Result<(), RepositoryError> {
        let connection = self.connection()?;
        renew_requested(&connection, id, expires_at_ms)
    }

    fn attach_multipart(
        &self,
        id: &str,
        provider_upload_id: &str,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        transfer_multipart::attach(self, id, provider_upload_id)
    }

    fn issue_upload_parts(
        &self,
        parts: &[blobyard_contract::NewUploadPartGrant],
    ) -> Result<Vec<blobyard_contract::UploadPartRecord>, RepositoryError> {
        transfer_multipart::issue(self, parts)
    }

    fn upload_part_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<(UploadReservationRecord, blobyard_contract::UploadPartRecord), RepositoryError>
    {
        transfer_multipart::by_capability(self, capability_hash, now_ms)
    }

    fn record_uploaded_part(
        &self,
        upload_id: &str,
        part_number: u32,
        size: u64,
        checksum: &str,
        provider_tag: Option<&str>,
    ) -> Result<(), RepositoryError> {
        transfer_multipart::record(self, upload_id, part_number, size, checksum, provider_tag)
    }

    fn list_upload_parts(
        &self,
        upload_id: &str,
    ) -> Result<Vec<blobyard_contract::UploadPartRecord>, RepositoryError> {
        transfer_multipart::list(self, upload_id)
    }

    fn record_uploaded_bytes(
        &self,
        id: &str,
        size: u64,
        checksum: &str,
    ) -> Result<(), RepositoryError> {
        validate_upload_integrity(id, checksum)?;
        let stored_size = to_i64(size)?;
        let connection = self.connection()?;
        let current = reservation_by_id(&connection, id)?;
        if current.expected_size != size || current.expected_checksum != checksum {
            return Err(RepositoryError::InvalidInput);
        }
        if current.state != ReservationState::Requested {
            return Err(RepositoryError::Conflict);
        }
        let changed = connection
            .execute(
                "UPDATE upload_reservations SET state = 'uploaded', received_size = ?2, received_checksum = ?3 WHERE id = ?1 AND state = 'requested'",
                params![id, stored_size, checksum],
            )
            .map_err(map_error)?;
        drop(connection);
        changed_once(changed)
    }

    fn complete_upload(&self, id: &str) -> Result<ObjectVersionRecord, RepositoryError> {
        rows::validate_text(id)?;
        self.write_transaction(|transaction| {
            let reservation = reservation_by_id(transaction, id)?;
            require_state(reservation.state, ReservationState::Uploaded)?;
            update_reservation_state(transaction, id, "uploaded", "complete", false)?;
            let changed = transaction
                .execute(
                    "UPDATE object_versions SET state = 'complete', size = (SELECT expected_size FROM upload_reservations WHERE id = ?2), checksum = ?3 WHERE id = ?1 AND state = 'pending'",
                    params![
                        reservation.version.id,
                        reservation.id,
                        reservation.expected_checksum
                    ],
                )
                .map_err(map_error)?;
            changed_once(changed)?;
            object_version_by_id(transaction, &reservation.version.id)
        })
    }

    fn abort_upload(&self, id: &str) -> Result<UploadReservationRecord, RepositoryError> {
        rows::validate_text(id)?;
        self.write_transaction(|transaction| {
            let reservation = reservation_by_id(transaction, id)?;
            if !matches!(
                reservation.state,
                ReservationState::Requested | ReservationState::Uploaded
            ) {
                return Err(RepositoryError::Conflict);
            }
            update_reservation_state(
                transaction,
                id,
                reservation.state.as_str(),
                "aborted",
                true,
            )?;
            transaction
                .execute("DELETE FROM upload_parts WHERE upload_id = ?1", [id])
                .map_err(map_error)?;
            let changed = transaction
                .execute(
                    "UPDATE object_versions SET state = 'aborted' WHERE id = ?1 AND state = 'pending'",
                    [&reservation.version.id],
                )
                .map_err(map_error)?;
            changed_once(changed)?;
            Ok(reservation)
        })
    }

    fn list_stored_objects(
        &self,
        project_id: &str,
        prefix: Option<&str>,
        include_versions: bool,
    ) -> Result<Vec<StoredObjectRecord>, RepositoryError> {
        transfer_listing::list(self, project_id, prefix, include_versions)
    }

    fn issue_download(&self, grant: &NewDownloadGrant) -> Result<(), RepositoryError> {
        transfer_downloads::issue(self, grant)
    }

    fn download_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<StoredObjectRecord, RepositoryError> {
        transfer_downloads::resolve(self, capability_hash, now_ms)
    }
}

pub(super) fn object_version_by_id(
    connection: &Connection,
    id: &str,
) -> Result<ObjectVersionRecord, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM object_versions WHERE id = ?1",
                rows::OBJECT_VERSION_COLUMNS
            ),
            [id],
            rows::object_version,
        )
        .map_err(map_error)
}

pub(super) fn update_reservation_state(
    transaction: &Transaction<'_>,
    id: &str,
    from: &str,
    to: &str,
    clear_received: bool,
) -> Result<(), RepositoryError> {
    let changed = if clear_received {
        transaction.execute(
            "UPDATE upload_reservations SET state = ?3, received_size = NULL, received_checksum = NULL WHERE id = ?1 AND state = ?2",
            params![id, from, to],
        )
    } else {
        transaction.execute(
            "UPDATE upload_reservations SET state = ?3 WHERE id = ?1 AND state = ?2",
            params![id, from, to],
        )
    }
    .map_err(map_error)?;
    changed_once(changed)
}

pub(super) fn require_state(
    actual: ReservationState,
    expected: ReservationState,
) -> Result<(), RepositoryError> {
    if actual == expected {
        Ok(())
    } else {
        Err(RepositoryError::Conflict)
    }
}
