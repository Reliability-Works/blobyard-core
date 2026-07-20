use crate::transport::RequestBody;
use crate::{S3Storage, StagedRead, StagedUpload};
use blobyard_contract::{ObjectChecksum, StorageError, StorageMetadata};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::Path;
use tempfile::NamedTempFile;

impl S3Storage {
    pub(crate) fn byte_stream(path: std::path::PathBuf) -> crate::BodyFuture {
        Box::pin(async move { Ok(RequestBody::File(path)) })
    }

    pub(crate) fn stage_upload(
        directory: &Path,
        source: &mut dyn Read,
    ) -> Result<StagedUpload, StorageError> {
        let mut temporary =
            NamedTempFile::new_in(directory).map_err(|_error| StorageError::Unavailable)?;
        copy_and_hash(source, &mut temporary).and_then(|metadata| {
            flush_writer(&mut temporary).map(|()| StagedUpload {
                temporary,
                metadata,
            })
        })
    }

    pub(crate) fn empty_download(directory: &Path) -> Result<StagedRead, StorageError> {
        let temporary =
            NamedTempFile::new_in(directory).map_err(|_error| StorageError::Unavailable)?;
        StagedRead::open(temporary)
    }

    pub(crate) fn hash_path(path: &Path) -> Result<StorageMetadata, StorageError> {
        File::open(path)
            .map_err(|_error| StorageError::Unavailable)
            .and_then(|mut file| copy_and_hash(&mut file, &mut std::io::sink()))
    }
}

impl StagedRead {
    pub(crate) fn open(temporary: NamedTempFile) -> Result<Self, StorageError> {
        temporary
            .reopen()
            .map_err(|_error| StorageError::Unavailable)
            .and_then(|mut file| {
                rewind_reader(&mut file).map(|()| Self {
                    file,
                    _temporary: temporary,
                })
            })
    }
}

impl Read for StagedRead {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buffer)
    }
}

fn copy_and_hash(
    source: &mut dyn Read,
    target: &mut dyn Write,
) -> Result<StorageMetadata, StorageError> {
    copy_and_hash_from_size(source, target, 0)
}

fn copy_and_hash_from_size(
    source: &mut dyn Read,
    target: &mut dyn Write,
    mut size: u64,
) -> Result<StorageMetadata, StorageError> {
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = source
            .read(&mut buffer)
            .map_err(|_error| StorageError::Unavailable)?;
        if count == 0 {
            let checksum = ObjectChecksum::from_sha256_digest(digest.finalize().into());
            return Ok(StorageMetadata { size, checksum });
        }
        target
            .write_all(&buffer[..count])
            .map_err(|_error| StorageError::Unavailable)?;
        digest.update(&buffer[..count]);
        size = size
            .checked_add(count as u64)
            .ok_or(StorageError::InvalidInput)?;
    }
}

fn flush_writer(target: &mut dyn Write) -> Result<(), StorageError> {
    target.flush().map_err(|_error| StorageError::Unavailable)
}

fn rewind_reader(target: &mut dyn Seek) -> Result<(), StorageError> {
    target.rewind().map_err(|_error| StorageError::Unavailable)
}

#[cfg(test)]
#[path = "tests/staging.rs"]
mod tests;
