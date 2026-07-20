#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{FilesystemStorage, checked_total_size, validate_parts};
use blobyard_contract::{
    MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, StorageError, StorageKey,
    StorageMetadata,
};
use std::io::Cursor;

fn expected() -> StorageMetadata {
    StorageMetadata {
        size: 4,
        checksum: ObjectChecksum::new("a".repeat(64)).expect("checksum"),
    }
}

#[test]
fn multipart_creation_reports_collisions_and_provider_failures() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let key = StorageKey::new("artifact.bin").expect("key");
    let id = MultipartId("fixed-upload".to_owned());
    assert_eq!(storage.create_multipart(&key, id.clone()), Ok(id.clone()));
    assert_eq!(
        storage.create_multipart(&key, id),
        Err(StorageError::Conflict)
    );

    let broken = tempfile::tempdir().expect("broken root");
    let broken_storage = FilesystemStorage::open(broken.path()).expect("broken storage");
    std::fs::remove_dir(broken.path().join("multipart")).expect("empty multipart root");
    std::fs::write(broken.path().join("multipart"), b"not a directory").expect("multipart blocker");
    assert_eq!(
        broken_storage.create_multipart(&key, MultipartId("upload".to_owned())),
        Err(StorageError::Unavailable)
    );
}

#[test]
fn multipart_guards_reject_invalid_sequences_and_size_overflow() {
    assert_eq!(validate_parts(&[]), Err(StorageError::InvalidInput));
    let checksum = ObjectChecksum::new("a".repeat(64)).expect("checksum");
    let noncontiguous = [MultipartPart {
        number: 2,
        size: 1,
        checksum,
        provider_tag: None,
    }];
    assert_eq!(
        validate_parts(&noncontiguous),
        Err(StorageError::InvalidInput)
    );
    assert_eq!(checked_total_size(4, 5), Ok(9));
    assert_eq!(
        checked_total_size(u64::MAX, 1),
        Err(StorageError::InvalidInput)
    );
}

#[test]
fn multipart_part_provider_failures_map_to_stable_errors() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let key = StorageKey::new("artifact.bin").expect("key");
    let upload = storage.begin_multipart(&key, &expected()).expect("upload");
    let directory = temporary.path().join("multipart").join(&upload.0);
    std::fs::create_dir(directory.join("00001.part")).expect("part blocker");
    assert_eq!(
        storage.put_part(&upload, 1, &mut Cursor::new(b"payload")),
        Err(StorageError::Unavailable)
    );
    std::fs::remove_dir(directory.join("00001.part")).expect("remove blocker");

    let missing = MultipartPart {
        number: 1,
        size: 1,
        checksum: ObjectChecksum::new("a".repeat(64)).expect("checksum"),
        provider_tag: None,
    };
    assert_eq!(
        storage.complete_multipart(&upload, &[missing]),
        Err(StorageError::NotFound)
    );
}
