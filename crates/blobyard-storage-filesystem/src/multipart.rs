use super::FilesystemStorage;
use blobyard_contract::{
    MultipartId, MultipartPart, ObjectChecksum, StorageError, StorageKey, StorageMetadata,
};
use sha2::Digest;
use std::fs;
use std::io::{Read, Write};

impl FilesystemStorage {
    pub(crate) fn start_multipart(&self, key: &StorageKey) -> Result<MultipartId, StorageError> {
        let id = MultipartId(uuid::Uuid::new_v4().to_string());
        self.create_multipart(key, id)
    }

    fn create_multipart(
        &self,
        key: &StorageKey,
        id: MultipartId,
    ) -> Result<MultipartId, StorageError> {
        let directory = self.paths.multipart().join(&id.0);
        fs::create_dir(&directory)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::AlreadyExists => StorageError::Conflict,
                _ => StorageError::Unavailable,
            })
            .and_then(|()| write_key(&directory.join("key"), key))
            .map(|()| id)
    }

    pub(crate) fn store_part(
        &self,
        upload: &MultipartId,
        number: u32,
        source: &mut dyn Read,
    ) -> Result<MultipartPart, StorageError> {
        if number == 0 || number > 10_000 {
            return Err(StorageError::InvalidInput);
        }
        let directory = self.existing_upload(upload)?;
        let path = directory.join(format!("{number:05}.part"));
        let metadata = Self::write_stream(&path, source, None, true)?;
        Ok(MultipartPart {
            number,
            size: metadata.size,
            checksum: metadata.checksum,
            provider_tag: None,
        })
    }

    pub(crate) fn finish_multipart(
        &self,
        upload: &MultipartId,
        parts: &[MultipartPart],
    ) -> Result<StorageMetadata, StorageError> {
        validate_parts(parts)?;
        let directory = self.existing_upload(upload)?;
        let key = read_key(&directory)?;
        let mut readers: Vec<Box<dyn Read>> = Vec::with_capacity(parts.len());
        for part in parts {
            let path = directory.join(format!("{:05}.part", part.number));
            readers.push(validated_reader(&path, part)?);
        }
        self.put_readers(&key, readers)
            .and_then(|metadata| remove_directory(&directory).map(|()| metadata))
    }

    pub(crate) fn cancel_multipart(&self, upload: &MultipartId) -> Result<(), StorageError> {
        let directory = self.existing_upload(upload)?;
        remove_directory(&directory)
    }

    pub(super) fn put_readers(
        &self,
        key: &StorageKey,
        readers: Vec<Box<dyn Read>>,
    ) -> Result<StorageMetadata, StorageError> {
        let target = self.paths.object(key);
        let parent = super::paths::secure_parent(&target)?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|_error| StorageError::Unavailable)?;
        let metadata = hash_readers(readers, &mut temporary)?;
        self.commit_temporary(key, temporary, metadata)
    }

    fn existing_upload(&self, id: &MultipartId) -> Result<std::path::PathBuf, StorageError> {
        let path = self.paths.upload(id)?;
        if path.is_dir() {
            Ok(path)
        } else {
            Err(StorageError::NotFound)
        }
    }
}

fn validate_parts(parts: &[MultipartPart]) -> Result<(), StorageError> {
    if parts.is_empty()
        || parts
            .iter()
            .enumerate()
            .any(|(index, part)| part.number != u32::try_from(index + 1).unwrap_or(u32::MAX))
    {
        Err(StorageError::InvalidInput)
    } else {
        Ok(())
    }
}

fn read_key(directory: &std::path::Path) -> Result<StorageKey, StorageError> {
    let value = fs::read_to_string(directory.join("key")).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => StorageError::NotFound,
        _ => StorageError::Unavailable,
    })?;
    StorageKey::new(value)
}

fn validated_reader(
    path: &std::path::Path,
    expected: &MultipartPart,
) -> Result<Box<dyn Read>, StorageError> {
    FilesystemStorage::hash_file(path).and_then(|metadata| {
        if metadata.size != expected.size || metadata.checksum != expected.checksum {
            Err(StorageError::IntegrityMismatch)
        } else {
            super::open_file(path).map(|file| Box::new(file) as Box<dyn Read>)
        }
    })
}

pub(super) fn write_key(path: &std::path::Path, key: &StorageKey) -> Result<(), StorageError> {
    fs::write(path, key.as_str()).map_err(|_error| StorageError::Unavailable)
}

pub(super) fn remove_directory(path: &std::path::Path) -> Result<(), StorageError> {
    fs::remove_dir_all(path).map_err(|_error| StorageError::Unavailable)
}

pub(super) fn append_reader(
    total: u64,
    reader: &mut dyn Read,
    target: &mut dyn Write,
    digest: &mut sha2::Sha256,
) -> Result<u64, StorageError> {
    let part_size = super::copy_hashed(reader, target, digest)?;
    checked_total_size(total, part_size)
}

pub(super) fn hash_readers(
    readers: Vec<Box<dyn Read>>,
    target: &mut dyn Write,
) -> Result<StorageMetadata, StorageError> {
    let mut digest = sha2::Sha256::new();
    let mut size = 0_u64;
    for mut reader in readers {
        size = append_reader(size, &mut reader, target, &mut digest)?;
    }
    super::flush_writer(target)?;
    let checksum = ObjectChecksum::from_sha256_digest(digest.finalize().into());
    Ok(StorageMetadata { size, checksum })
}

fn checked_total_size(total: u64, part: u64) -> Result<u64, StorageError> {
    total.checked_add(part).ok_or(StorageError::InvalidInput)
}

#[cfg(test)]
#[path = "multipart_tests.rs"]
mod tests;
