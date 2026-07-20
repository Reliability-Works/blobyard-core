use crate::api::AppState;
use crate::error::ApiError;
use blobyard_api_client::CompletedPart;
use blobyard_contract::{
    MultipartId, MultipartPart, ObjectChecksum, ReservationStrategy, StorageError, StorageKey,
    StorageMetadata, UploadPartRecord, UploadReservationRecord,
};

pub(crate) fn ensure_provider(
    state: &AppState,
    reservation: UploadReservationRecord,
) -> Result<UploadReservationRecord, ApiError> {
    if reservation.strategy == ReservationStrategy::Single
        || reservation.provider_upload_id.is_some()
    {
        return Ok(reservation);
    }
    let key = ApiError::internal_result(StorageKey::new(reservation.version.storage_key.clone()))?;
    let expected = StorageMetadata {
        size: reservation.expected_size,
        checksum: ObjectChecksum::new(reservation.expected_checksum.clone())
            .map_err(ApiError::from_storage)?,
    };
    let created = state
        .storage
        .begin_multipart(&key, &expected)
        .map_err(ApiError::from_storage)?;
    match state
        .repository
        .attach_multipart(&reservation.id, &created.0)
    {
        Ok(attached) => Ok(attached),
        Err(error) => {
            cleanup_created(state, &created)?;
            if error == blobyard_contract::RepositoryError::Conflict {
                state
                    .repository
                    .upload_by_id(&reservation.id)
                    .map_err(ApiError::from_repository)
                    .and_then(require_provider)
            } else {
                Err(ApiError::from_repository(error))
            }
        }
    }
}

fn cleanup_created(state: &AppState, upload: &MultipartId) -> Result<(), ApiError> {
    match state.storage.abort_multipart(upload) {
        Ok(()) | Err(StorageError::NotFound) => Ok(()),
        Err(error) => Err(ApiError::from_storage(error)),
    }
}

fn require_provider(
    reservation: UploadReservationRecord,
) -> Result<UploadReservationRecord, ApiError> {
    if reservation.strategy == ReservationStrategy::Multipart
        && reservation.provider_upload_id.is_some()
    {
        Ok(reservation)
    } else {
        Err(ApiError::conflict())
    }
}

pub(crate) fn complete(
    state: &AppState,
    upload: &UploadReservationRecord,
    submitted: &[CompletedPart],
) -> Result<(), ApiError> {
    if upload.strategy == ReservationStrategy::Single {
        return if submitted.is_empty() {
            Ok(())
        } else {
            Err(ApiError::invalid_request())
        };
    }
    let stored = state
        .repository
        .list_upload_parts(&upload.id)
        .map_err(ApiError::from_repository)?;
    let parts = completion_parts(upload, submitted, &stored)?;
    let provider = upload
        .provider_upload_id
        .clone()
        .map(MultipartId)
        .ok_or_else(ApiError::conflict)?;
    let metadata = match state.storage.complete_multipart(&provider, &parts) {
        Ok(metadata) => metadata,
        Err(StorageError::NotFound) => existing_metadata(state, upload)?,
        Err(error) => return Err(ApiError::from_storage(error)),
    };
    verify_complete(upload, &metadata)?;
    state
        .repository
        .record_uploaded_bytes(&upload.id, metadata.size, metadata.checksum.as_str())
        .map_err(ApiError::from_repository)
}

fn completion_parts(
    upload: &UploadReservationRecord,
    submitted: &[CompletedPart],
    stored: &[UploadPartRecord],
) -> Result<Vec<MultipartPart>, ApiError> {
    let count = upload.part_count.ok_or_else(ApiError::conflict)?;
    if count > 10_000 {
        return Err(ApiError::conflict());
    }
    let expected_count = count as usize;
    if submitted.len() != expected_count || stored.len() != expected_count {
        return Err(ApiError::conflict());
    }
    submitted
        .iter()
        .zip(stored)
        .zip(1..=count)
        .map(|((client, record), number)| {
            let checksum = record
                .received_checksum
                .as_ref()
                .ok_or_else(ApiError::conflict)?;
            let size = record.received_size.ok_or_else(ApiError::conflict)?;
            if client.part_number != number
                || record.part_number != number
                || client.etag != format!("\"{checksum}\"")
                || size != record.expected_size
            {
                return Err(ApiError::invalid_request());
            }
            Ok(MultipartPart {
                number,
                size,
                checksum: ObjectChecksum::new(checksum.clone()).map_err(ApiError::from_storage)?,
                provider_tag: record.provider_tag.clone(),
            })
        })
        .collect()
}

fn existing_metadata(
    state: &AppState,
    upload: &UploadReservationRecord,
) -> Result<StorageMetadata, ApiError> {
    let key = ApiError::internal_result(StorageKey::new(upload.version.storage_key.clone()))?;
    state.storage.head(&key).map_err(ApiError::from_storage)
}

fn verify_complete(
    upload: &UploadReservationRecord,
    metadata: &StorageMetadata,
) -> Result<(), ApiError> {
    if metadata.size == upload.expected_size
        && metadata.checksum.as_str() == upload.expected_checksum
    {
        Ok(())
    } else {
        Err(ApiError::invalid_request())
    }
}

pub(crate) fn abort_storage(
    state: &AppState,
    upload: &UploadReservationRecord,
) -> Result<(), ApiError> {
    if let Some(provider) = upload.provider_upload_id.clone().map(MultipartId) {
        ignore_missing(state.storage.abort_multipart(&provider))?;
    }
    let key = ApiError::internal_result(StorageKey::new(upload.version.storage_key.clone()))?;
    ignore_missing(state.storage.delete(&key))
}

const fn ignore_missing(result: Result<(), StorageError>) -> Result<(), ApiError> {
    match result {
        Ok(()) | Err(StorageError::NotFound) => Ok(()),
        Err(error) => Err(ApiError::from_storage(error)),
    }
}

pub(crate) fn completed_part_numbers(
    state: &AppState,
    upload: &UploadReservationRecord,
) -> Result<Vec<u32>, ApiError> {
    if upload.strategy == ReservationStrategy::Single {
        return Ok(Vec::new());
    }
    state
        .repository
        .list_upload_parts(&upload.id)
        .map_err(ApiError::from_repository)
        .map(|parts| {
            parts
                .into_iter()
                .filter(|part| part.received_checksum.is_some())
                .map(|part| part.part_number)
                .collect()
        })
}

#[cfg(test)]
#[path = "transfer_multipart_unit_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "transfer_multipart_failure_tests.rs"]
mod failure_tests;
