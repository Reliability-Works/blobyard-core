use super::{
    FilesystemStorage, checked_size, copy_flush_and_hash, copy_hashed_from, flush_writer,
    multipart, open_file, paths, seek_to, write_bytes,
};
use blobyard_contract::{StorageError, StorageKey, StorageMetadata};
use sha2::Digest;
use std::io::{Read, Seek, Write};
use std::path::Path;

/// Flushes through the production storage error mapper.
pub fn flush(target: &mut dyn Write) -> Result<(), StorageError> {
    flush_writer(target)
}

/// Writes and flushes through the production storage error mapper.
pub fn write(target: &mut dyn Write, bytes: &[u8]) -> Result<(), StorageError> {
    write_bytes(target, bytes)
}

/// Opens a file through the production storage error mapper.
pub fn open(path: &Path) -> Result<(), StorageError> {
    open_file(path).map(drop)
}

/// Seeks through the production storage error mapper.
pub fn seek(target: &mut dyn Seek, start: u64) -> Result<(), StorageError> {
    seek_to(target, start)
}

/// Adds a bounded read count through the production overflow guard.
pub fn add_size(total: u64, count: usize) -> Result<u64, StorageError> {
    checked_size(total, count)
}

/// Copies and flushes through the production whole-object streaming core.
pub fn copy_and_flush(
    source: &mut dyn Read,
    target: &mut dyn Write,
) -> Result<StorageMetadata, StorageError> {
    copy_flush_and_hash(source, target)
}

/// Copies from an explicit size through the production streaming overflow guard.
pub fn copy_from_size(
    total: u64,
    source: &mut dyn Read,
    target: &mut dyn Write,
) -> Result<u64, StorageError> {
    let mut digest = sha2::Sha256::new();
    copy_hashed_from(total, source, target, &mut digest)
}

/// Canonicalizes through the production storage error mapper.
pub fn canonicalize(path: &Path) -> Result<(), StorageError> {
    paths::canonicalize_directory(path).map(drop)
}

/// Writes one multipart key through the production storage error mapper.
pub fn write_multipart_key(path: &Path, key: &StorageKey) -> Result<(), StorageError> {
    multipart::write_key(path, key)
}

/// Removes a multipart directory through the production storage error mapper.
pub fn remove_multipart_directory(path: &Path) -> Result<(), StorageError> {
    multipart::remove_directory(path)
}

/// Appends one multipart reader through the production hash and overflow guards.
pub fn append_multipart_reader(
    total: u64,
    reader: &mut dyn Read,
    target: &mut dyn Write,
) -> Result<u64, StorageError> {
    let mut digest = sha2::Sha256::new();
    multipart::append_reader(total, reader, target, &mut digest)
}

/// Hashes multipart readers through the production streaming core.
pub fn hash_multipart_readers(
    readers: Vec<Box<dyn Read>>,
    target: &mut dyn Write,
) -> Result<StorageMetadata, StorageError> {
    multipart::hash_readers(readers, target)
}

/// Commits type-erased multipart readers through the production adapter core.
pub fn put_multipart_readers(
    storage: &FilesystemStorage,
    key: &StorageKey,
    readers: Vec<Box<dyn Read>>,
) -> Result<StorageMetadata, StorageError> {
    storage.put_readers(key, readers)
}
