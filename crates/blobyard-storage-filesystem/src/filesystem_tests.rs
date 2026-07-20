#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{FilesystemStorage, copy_and_hash, copy_hashed, map_io, map_persist_error};
use blobyard_contract::{ObjectChecksum, ObjectStorage, StorageError, StorageKey, StorageMetadata};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Error, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug)]
struct FailingReader {
    failure: std::io::ErrorKind,
}

impl Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Err(Error::from(self.failure))
    }
}

fn failing_reader() -> FailingReader {
    FailingReader {
        failure: std::io::ErrorKind::UnexpectedEof,
    }
}

#[derive(Debug)]
struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        Err(Error::other("fixture write failure"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn io_errors_map_to_stable_storage_classes() {
    assert_eq!(
        map_io(std::io::Error::from(std::io::ErrorKind::NotFound)),
        StorageError::NotFound
    );
    assert_eq!(
        map_io(std::io::Error::from(std::io::ErrorKind::AlreadyExists)),
        StorageError::Conflict
    );
    assert_eq!(
        map_io(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        StorageError::Unavailable
    );
}

#[test]
fn persistence_errors_distinguish_conflicts_from_provider_failures() {
    let conflict = std::io::Error::from(std::io::ErrorKind::AlreadyExists);
    let unavailable = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    assert_eq!(map_persist_error(&conflict), StorageError::Conflict);
    assert_eq!(map_persist_error(&unavailable), StorageError::Unavailable);
}

#[test]
fn stream_copy_propagates_reader_and_writer_failures() {
    let mut sink = Vec::new();
    assert_eq!(
        copy_and_hash(&mut failing_reader(), &mut sink),
        Err(StorageError::Unavailable)
    );
    let mut digest = Sha256::new();
    assert_eq!(
        copy_hashed(
            &mut Cursor::new(b"payload"),
            &mut FailingWriter,
            &mut digest
        ),
        Err(StorageError::Unavailable)
    );
    assert_eq!(digest.finalize().as_slice(), Sha256::digest([]).as_slice());
}

#[test]
fn replacing_a_directory_and_recommitting_an_object_fail_closed() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let target = std::fs::canonicalize(temporary.path())
        .expect("canonical temporary directory")
        .join("directory-target");
    std::fs::create_dir(&target).expect("directory target");
    assert_eq!(
        FilesystemStorage::write_stream(&target, &mut Cursor::new(b"payload"), None, true),
        Err(StorageError::Unavailable)
    );

    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let key = StorageKey::new("existing.bin").expect("key");
    let metadata = storage
        .put(&key, &mut Cursor::new(b"payload"), None)
        .expect("stored object");
    let staged =
        tempfile::NamedTempFile::new_in(temporary.path().join("objects")).expect("staged object");
    assert_eq!(
        storage.commit_temporary(&key, staged, metadata),
        Err(StorageError::Conflict)
    );
}

#[test]
fn metadata_reader_rejects_invalid_json_and_checksum() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let key = StorageKey::new("metadata.bin").expect("key");
    storage
        .put(&key, &mut Cursor::new(b"payload"), None)
        .expect("stored object");
    let mut metadata_path = temporary.path().join("metadata").join(key.as_str());
    metadata_path.set_extension("bin.blobyard-meta");

    std::fs::write(&metadata_path, b"not json").expect("corrupt JSON");
    assert_eq!(storage.head(&key), Err(StorageError::IntegrityMismatch));
    std::fs::write(&metadata_path, br#"{"size":7,"checksum":"invalid"}"#)
        .expect("invalid checksum");
    assert_eq!(storage.head(&key), Err(StorageError::IntegrityMismatch));
}

#[test]
fn metadata_write_failure_rolls_back_a_staged_object() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let key = StorageKey::new("blocked/metadata.bin").expect("key");
    let metadata_parent = temporary.path().join("metadata").join("blocked");
    std::fs::write(&metadata_parent, b"not a directory").expect("metadata blocker");
    assert_eq!(
        storage.put(&key, &mut Cursor::new(b"payload"), None),
        Err(StorageError::Unavailable)
    );
    assert!(!temporary.path().join("objects").join(key.as_str()).exists());
}

#[test]
fn checksum_and_metadata_value_fixture_is_well_formed() {
    let checksum = ObjectChecksum::new("a".repeat(64)).expect("checksum");
    assert_eq!(
        StorageMetadata { size: 7, checksum }.size,
        u64::try_from(b"payload".len()).expect("payload size")
    );
}

#[test]
fn missing_files_preserve_not_found_through_hash_and_metadata_cleanup() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    assert_eq!(
        FilesystemStorage::hash_file(&temporary.path().join("missing")),
        Err(StorageError::NotFound)
    );
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let key = StorageKey::new("cleanup.bin").expect("key");
    storage
        .put(&key, &mut Cursor::new(b"payload"), None)
        .expect("stored object");
    let mut metadata = temporary.path().join("metadata/cleanup.bin");
    metadata.set_extension("bin.blobyard-meta");
    std::fs::remove_file(metadata).expect("remove metadata");
    assert_eq!(storage.delete(&key), Err(StorageError::NotFound));
}

#[cfg(unix)]
#[test]
fn temporary_file_creation_failures_are_stable_and_rollback_bytes() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let locked = std::fs::canonicalize(temporary.path())
        .expect("canonical temporary directory")
        .join("locked");
    std::fs::create_dir(&locked).expect("locked directory");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555))
        .expect("lock directory");
    assert_eq!(
        FilesystemStorage::write_stream(
            &locked.join("object.bin"),
            &mut Cursor::new(b"payload"),
            None,
            false,
        ),
        Err(StorageError::Unavailable)
    );
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755))
        .expect("unlock directory");

    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let key = StorageKey::new("nested/object.bin").expect("key");
    let metadata_parent = temporary.path().join("metadata/nested");
    std::fs::create_dir(&metadata_parent).expect("metadata parent");
    std::fs::set_permissions(&metadata_parent, std::fs::Permissions::from_mode(0o555))
        .expect("lock metadata");
    assert_eq!(
        storage.put(&key, &mut Cursor::new(b"payload"), None),
        Err(StorageError::Unavailable)
    );
    assert!(!temporary.path().join("objects/nested/object.bin").exists());
    std::fs::set_permissions(&metadata_parent, std::fs::Permissions::from_mode(0o755))
        .expect("unlock metadata");
}
