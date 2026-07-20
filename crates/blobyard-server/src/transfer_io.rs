use crate::{api::AppState, error::ApiError};
use axum::body::Body;
use blobyard_contract::{
    MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, StorageError, StorageKey,
    StorageMetadata, UploadPartRecord, UploadReservationRecord,
};
use futures_util::StreamExt;
use tempfile::NamedTempFile;
use tokio::io::{AsyncWriteExt, BufWriter};

#[derive(Clone, Copy)]
struct StageHooks {
    reopen: fn(&NamedTempFile) -> std::io::Result<std::fs::File>,
    after_write: fn(std::io::Result<()>) -> std::io::Result<()>,
    after_flush: fn(std::io::Result<()>) -> std::io::Result<()>,
    after_sync: fn(std::io::Result<()>) -> std::io::Result<()>,
}

impl StageHooks {
    const PRODUCTION: Self = Self {
        reopen: NamedTempFile::reopen,
        after_write: preserve_io,
        after_flush: preserve_io,
        after_sync: preserve_io,
    };
}

const fn preserve_io(result: std::io::Result<()>) -> std::io::Result<()> {
    result
}

pub(crate) async fn receive(
    state: &AppState,
    reservation: &UploadReservationRecord,
    body: Body,
) -> Result<StorageMetadata, ApiError> {
    let temporary = stage_body(state, reservation.expected_size, body).await?;
    let storage = state.storage.clone();
    let key =
        StorageKey::new(reservation.version.storage_key.clone()).map_err(ApiError::from_storage)?;
    let checksum = ObjectChecksum::new(reservation.expected_checksum.clone())
        .map_err(ApiError::from_storage)?;
    ApiError::internal_result(
        tokio::task::spawn_blocking(move || store(storage.as_ref(), &key, &checksum, &temporary))
            .await,
    )?
    .map_err(ApiError::from_storage)
}

pub(crate) async fn receive_part(
    state: &AppState,
    reservation: &UploadReservationRecord,
    part: &UploadPartRecord,
    body: Body,
) -> Result<MultipartPart, ApiError> {
    let temporary = stage_body(state, part.expected_size, body).await?;
    let storage = state.storage.clone();
    let upload = reservation
        .provider_upload_id
        .clone()
        .map(MultipartId)
        .ok_or_else(ApiError::internal)?;
    let number = part.part_number;
    ApiError::internal_result(
        tokio::task::spawn_blocking(move || {
            store_part(storage.as_ref(), &upload, number, &temporary)
        })
        .await,
    )?
    .map_err(ApiError::from_storage)
}

async fn stage_body(
    state: &AppState,
    expected_size: u64,
    body: Body,
) -> Result<NamedTempFile, ApiError> {
    stage_body_with(state, expected_size, body, StageHooks::PRODUCTION).await
}

async fn stage_body_with(
    state: &AppState,
    expected_size: u64,
    body: Body,
    hooks: StageHooks,
) -> Result<NamedTempFile, ApiError> {
    let temporary =
        NamedTempFile::new_in(&state.staging_directory).map_err(|_error| ApiError::internal())?;
    let output = ApiError::internal_result((hooks.reopen)(&temporary))?;
    let mut output = BufWriter::new(tokio::fs::File::from_std(output));
    let mut stream = body.into_data_stream();
    let mut received = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_error| ApiError::invalid_request())?;
        let chunk_size = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        if chunk_size > expected_size.saturating_sub(received) {
            return Err(ApiError::invalid_request());
        }
        received += chunk_size;
        let written = (hooks.after_write)(output.write_all(&chunk).await);
        ApiError::internal_result(written)?;
    }
    if received != expected_size {
        return Err(ApiError::invalid_request());
    }
    let flushed = (hooks.after_flush)(output.flush().await);
    ApiError::internal_result(flushed)?;
    let synced = (hooks.after_sync)(output.get_ref().sync_all().await);
    ApiError::internal_result(synced)?;
    drop(output);
    Ok(temporary)
}

fn store(
    storage: &dyn ObjectStorage,
    key: &StorageKey,
    checksum: &ObjectChecksum,
    temporary: &NamedTempFile,
) -> Result<StorageMetadata, StorageError> {
    let mut source = temporary
        .reopen()
        .map_err(|_error| StorageError::Unavailable)?;
    match storage.put(key, &mut source, Some(checksum)) {
        Ok(metadata) => Ok(metadata),
        Err(StorageError::Conflict) => existing(storage, key, checksum, temporary),
        Err(error) => Err(error),
    }
}

fn store_part(
    storage: &dyn ObjectStorage,
    upload: &MultipartId,
    number: u32,
    temporary: &NamedTempFile,
) -> Result<MultipartPart, StorageError> {
    let mut source = temporary
        .reopen()
        .map_err(|_error| StorageError::Unavailable)?;
    storage.put_part(upload, number, &mut source)
}

fn existing(
    storage: &dyn ObjectStorage,
    key: &StorageKey,
    checksum: &ObjectChecksum,
    temporary: &NamedTempFile,
) -> Result<StorageMetadata, StorageError> {
    existing_with(storage, key, checksum, temporary, staged_metadata)
}

fn existing_with(
    storage: &dyn ObjectStorage,
    key: &StorageKey,
    checksum: &ObjectChecksum,
    temporary: &NamedTempFile,
    metadata: fn(&NamedTempFile) -> std::io::Result<std::fs::Metadata>,
) -> Result<StorageMetadata, StorageError> {
    let stored = storage.head(key)?;
    let source_size = metadata(temporary)
        .map_err(|_error| StorageError::Unavailable)?
        .len();
    if stored.size == source_size && &stored.checksum == checksum {
        Ok(stored)
    } else {
        Err(StorageError::IntegrityMismatch)
    }
}

fn staged_metadata(temporary: &NamedTempFile) -> std::io::Result<std::fs::Metadata> {
    temporary.as_file().metadata()
}

#[cfg(test)]
#[path = "transfer_io_tests.rs"]
mod tests;
