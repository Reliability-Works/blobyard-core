#[path = "inventory.rs"]
mod inventory;
#[path = "multipart.rs"]
mod multipart;
#[path = "paths.rs"]
mod paths;

use blobyard_contract::{
    ByteRange, MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, StorageError, StorageKey,
    StorageMetadata, StorageRead,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use tempfile::NamedTempFile;

/// Filesystem adapter rooted beneath one private durable directory.
#[derive(Debug)]
pub struct FilesystemStorage {
    paths: paths::StoragePaths,
}

impl FilesystemStorage {
    /// Creates or opens a filesystem storage root.
    ///
    /// # Errors
    ///
    /// Returns a stable storage error when the root cannot be created safely.
    pub fn open(root: &Path) -> Result<Self, StorageError> {
        Ok(Self {
            paths: paths::StoragePaths::create(root)?,
        })
    }

    pub(super) fn write_stream(
        target: &Path,
        source: &mut dyn Read,
        expected: Option<&ObjectChecksum>,
        replace: bool,
    ) -> Result<StorageMetadata, StorageError> {
        let parent = paths::secure_parent(target)?;
        let mut temporary =
            NamedTempFile::new_in(parent).map_err(|_error| StorageError::Unavailable)?;
        let metadata = copy_flush_and_hash(source, &mut temporary)?;
        if expected.is_some_and(|checksum| checksum != &metadata.checksum) {
            return Err(StorageError::IntegrityMismatch);
        }
        if replace {
            temporary
                .persist(target)
                .map_err(|_error| StorageError::Unavailable)?;
        } else {
            temporary
                .persist_noclobber(target)
                .map_err(|error| map_persist_error(&error.error))?;
        }
        Ok(metadata)
    }

    pub(super) fn hash_file(path: &Path) -> Result<StorageMetadata, StorageError> {
        let mut source = open_file(path)?;
        copy_and_hash(&mut source, &mut std::io::sink())
    }

    pub(super) fn commit_temporary(
        &self,
        key: &StorageKey,
        temporary: NamedTempFile,
        metadata: StorageMetadata,
    ) -> Result<StorageMetadata, StorageError> {
        let target = self.paths.object(key);
        temporary
            .persist_noclobber(&target)
            .map_err(|error| map_persist_error(&error.error))?;
        if let Err(error) = self.write_metadata(key, &metadata) {
            let _ignored = fs::remove_file(target);
            return Err(error);
        }
        Ok(metadata)
    }

    fn write_metadata(
        &self,
        key: &StorageKey,
        metadata: &StorageMetadata,
    ) -> Result<(), StorageError> {
        let target = self.paths.metadata(key);
        let parent = paths::secure_parent(&target)?;
        let wire = MetadataFile::from(metadata);
        let bytes = wire.encode();
        let mut temporary =
            NamedTempFile::new_in(parent).map_err(|_error| StorageError::Unavailable)?;
        write_bytes(&mut temporary, &bytes).and_then(|()| {
            temporary
                .persist_noclobber(target)
                .map(|_file| ())
                .map_err(|_error| StorageError::Conflict)
        })
    }

    fn read_metadata(&self, key: &StorageKey) -> Result<StorageMetadata, StorageError> {
        let bytes = fs::read(self.paths.metadata(key)).map_err(map_io)?;
        let wire: MetadataFile =
            serde_json::from_slice(&bytes).map_err(|_error| StorageError::IntegrityMismatch)?;
        let checksum =
            ObjectChecksum::new(wire.checksum).map_err(|_error| StorageError::IntegrityMismatch)?;
        Ok(StorageMetadata {
            size: wire.size,
            checksum,
        })
    }
}

impl ObjectStorage for FilesystemStorage {
    fn put(
        &self,
        key: &StorageKey,
        source: &mut dyn Read,
        expected: Option<&ObjectChecksum>,
    ) -> Result<StorageMetadata, StorageError> {
        let target = self.paths.object(key);
        let metadata = Self::write_stream(&target, source, expected, false)?;
        if let Err(error) = self.write_metadata(key, &metadata) {
            let _ignored = fs::remove_file(target);
            return Err(error);
        }
        Ok(metadata)
    }

    fn get(&self, key: &StorageKey, range: Option<ByteRange>) -> Result<StorageRead, StorageError> {
        let metadata = self.head(key)?;
        let range = range.unwrap_or(ByteRange {
            start: 0,
            end: metadata.size,
        });
        if range.start > range.end {
            return Err(StorageError::InvalidInput);
        }
        if range.end > metadata.size {
            return Err(StorageError::InvalidInput);
        }
        open_file(&self.paths.object(key))
            .and_then(|mut file| seek_to(&mut file, range.start).map(|()| file))
            .map(|file| StorageRead {
                reader: Box::new(file.take(range.end - range.start)),
                metadata,
                range,
            })
    }

    fn head(&self, key: &StorageKey) -> Result<StorageMetadata, StorageError> {
        let metadata = self.read_metadata(key)?;
        let actual = fs::metadata(self.paths.object(key)).map_err(map_io)?;
        if !actual.is_file() || actual.len() != metadata.size {
            Err(StorageError::IntegrityMismatch)
        } else {
            Ok(metadata)
        }
    }

    fn delete(&self, key: &StorageKey) -> Result<(), StorageError> {
        fs::remove_file(self.paths.object(key)).map_err(map_io)?;
        fs::remove_file(self.paths.metadata(key)).map_err(map_io)
    }

    fn begin_multipart(
        &self,
        key: &StorageKey,
        _expected: &StorageMetadata,
    ) -> Result<MultipartId, StorageError> {
        self.start_multipart(key)
    }

    fn put_part(
        &self,
        upload: &MultipartId,
        number: u32,
        source: &mut dyn Read,
    ) -> Result<MultipartPart, StorageError> {
        self.store_part(upload, number, source)
    }

    fn complete_multipart(
        &self,
        upload: &MultipartId,
        parts: &[MultipartPart],
    ) -> Result<StorageMetadata, StorageError> {
        self.finish_multipart(upload, parts)
    }

    fn abort_multipart(&self, upload: &MultipartId) -> Result<(), StorageError> {
        self.cancel_multipart(upload)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MetadataFile {
    size: u64,
    checksum: String,
}

impl MetadataFile {
    fn encode(&self) -> Vec<u8> {
        format!(r#"{{"size":{},"checksum":"{}"}}"#, self.size, self.checksum).into_bytes()
    }
}

impl From<&StorageMetadata> for MetadataFile {
    fn from(metadata: &StorageMetadata) -> Self {
        Self {
            size: metadata.size,
            checksum: metadata.checksum.as_str().to_owned(),
        }
    }
}

fn copy_and_hash(
    source: &mut dyn Read,
    target: &mut dyn Write,
) -> Result<StorageMetadata, StorageError> {
    let mut digest = Sha256::new();
    let size = copy_hashed(source, target, &mut digest)?;
    let checksum = ObjectChecksum::from_sha256_digest(digest.finalize().into());
    Ok(StorageMetadata { size, checksum })
}

fn copy_flush_and_hash(
    source: &mut dyn Read,
    target: &mut dyn Write,
) -> Result<StorageMetadata, StorageError> {
    let metadata = copy_and_hash(source, target)?;
    flush_writer(target)?;
    Ok(metadata)
}

fn copy_hashed(
    source: &mut dyn Read,
    target: &mut dyn Write,
    digest: &mut Sha256,
) -> Result<u64, StorageError> {
    copy_hashed_from(0, source, target, digest)
}

fn copy_hashed_from(
    mut size: u64,
    source: &mut dyn Read,
    target: &mut dyn Write,
    digest: &mut Sha256,
) -> Result<u64, StorageError> {
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = source.read(&mut buffer).map_err(map_io)?;
        if count == 0 {
            return Ok(size);
        }
        target.write_all(&buffer[..count]).map_err(map_io)?;
        digest.update(&buffer[..count]);
        size = checked_size(size, count)?;
    }
}

fn checked_size(total: u64, count: usize) -> Result<u64, StorageError> {
    total
        .checked_add(count as u64)
        .ok_or(StorageError::InvalidInput)
}

fn flush_writer(target: &mut dyn Write) -> Result<(), StorageError> {
    target.flush().map_err(|_error| StorageError::Unavailable)
}

fn write_bytes(target: &mut dyn Write, bytes: &[u8]) -> Result<(), StorageError> {
    target
        .write_all(bytes)
        .map_err(|_error| StorageError::Unavailable)?;
    flush_writer(target)
}

fn open_file(path: &Path) -> Result<File, StorageError> {
    File::open(path).map_err(map_io)
}

fn seek_to(target: &mut dyn Seek, start: u64) -> Result<(), StorageError> {
    target
        .seek(SeekFrom::Start(start))
        .map(|_position| ())
        .map_err(|_error| StorageError::Unavailable)
}

fn map_io(error: std::io::Error) -> StorageError {
    let kind = error.kind();
    drop(error);
    match kind {
        std::io::ErrorKind::NotFound => StorageError::NotFound,
        std::io::ErrorKind::AlreadyExists => StorageError::Conflict,
        _ => StorageError::Unavailable,
    }
}

fn map_persist_error(error: &std::io::Error) -> StorageError {
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        StorageError::Conflict
    } else {
        StorageError::Unavailable
    }
}

/// Test-only entry points for deterministic filesystem I/O failure contracts.
#[cfg(any(test, feature = "test-seams"))]
#[doc(hidden)]
#[path = "filesystem_seams.rs"]
pub mod test_seams;

#[cfg(test)]
#[path = "filesystem_tests.rs"]
mod tests;
