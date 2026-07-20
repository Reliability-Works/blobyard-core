#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Filesystem rollback, integrity, and path-safety edge contracts.

use blobyard_contract::{
    ByteRange, MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, StorageError, StorageKey,
    StorageMetadata,
};
use blobyard_storage_filesystem::FilesystemStorage;
use std::io::{Cursor, Error, Read};

#[derive(Debug)]
struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Err(Error::other("fixture read failure"))
    }
}

fn object_path(root: &std::path::Path, key: &StorageKey) -> std::path::PathBuf {
    root.join("objects").join(key.as_str())
}

fn metadata_path(root: &std::path::Path, key: &StorageKey) -> std::path::PathBuf {
    let mut path = root.join("metadata").join(key.as_str());
    path.set_extension(format!(
        "{}blobyard-meta",
        path.extension()
            .map_or_else(String::new, |value| format!("{}.", value.to_string_lossy()))
    ));
    path
}

fn expected_multipart() -> StorageMetadata {
    StorageMetadata {
        size: 4,
        checksum: ObjectChecksum::new("a".repeat(64)).expect("checksum"),
    }
}

#[test]
fn put_rolls_back_bytes_when_integrity_metadata_conflicts() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let key = StorageKey::new("releases/app.bin").expect("key");
    let metadata = metadata_path(temporary.path(), &key);
    std::fs::create_dir_all(metadata.parent().expect("metadata parent"))
        .expect("metadata directory");
    std::fs::write(&metadata, b"occupied").expect("metadata conflict fixture");

    assert_eq!(
        storage.put(&key, &mut Cursor::new(b"payload"), None),
        Err(StorageError::Conflict)
    );
    assert!(!object_path(temporary.path(), &key).exists());
    assert_eq!(
        std::fs::read(metadata).expect("existing metadata"),
        b"occupied"
    );
}

#[test]
fn reads_reject_invalid_ranges_and_corrupt_objects() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let key = StorageKey::new("objects/ranged.bin").expect("key");
    storage
        .put(&key, &mut Cursor::new(b"four"), None)
        .expect("stored object");

    assert!(matches!(
        storage.get(&key, Some(ByteRange { start: 3, end: 2 })),
        Err(StorageError::InvalidInput)
    ));
    assert!(matches!(
        storage.get(&key, Some(ByteRange { start: 0, end: 5 })),
        Err(StorageError::InvalidInput)
    ));

    let object = object_path(temporary.path(), &key);
    std::fs::write(&object, b"bad").expect("truncate object");
    assert_eq!(storage.head(&key), Err(StorageError::IntegrityMismatch));
    std::fs::remove_file(&object).expect("remove corrupt object");
    std::fs::create_dir(&object).expect("replace object with directory");
    assert_eq!(storage.head(&key), Err(StorageError::IntegrityMismatch));
}

#[test]
fn public_io_failures_preserve_provider_classes_and_cleanup() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let missing = StorageKey::new("missing.bin").expect("key");
    assert!(matches!(
        storage.get(&missing, None),
        Err(StorageError::NotFound)
    ));
    assert_eq!(storage.head(&missing), Err(StorageError::NotFound));
    assert_eq!(storage.delete(&missing), Err(StorageError::NotFound));

    let unreadable = StorageKey::new("unreadable.bin").expect("key");
    assert_eq!(
        storage.put(&unreadable, &mut FailingReader, None),
        Err(StorageError::Unavailable)
    );
    assert!(!object_path(temporary.path(), &unreadable).exists());

    let missing_bytes = StorageKey::new("missing-bytes.bin").expect("key");
    storage
        .put(&missing_bytes, &mut Cursor::new(b"payload"), None)
        .expect("stored object");
    std::fs::remove_file(object_path(temporary.path(), &missing_bytes)).expect("remove bytes");
    assert_eq!(storage.head(&missing_bytes), Err(StorageError::NotFound));

    let missing_metadata = StorageKey::new("missing-metadata.bin").expect("key");
    storage
        .put(&missing_metadata, &mut Cursor::new(b"payload"), None)
        .expect("stored object");
    std::fs::remove_file(metadata_path(temporary.path(), &missing_metadata))
        .expect("remove metadata");
    assert_eq!(
        storage.delete(&missing_metadata),
        Err(StorageError::NotFound)
    );

    let blocked_metadata = StorageKey::new("blocked/metadata.bin").expect("key");
    std::fs::write(
        temporary.path().join("metadata/blocked"),
        b"not a directory",
    )
    .expect("metadata blocker");
    assert_eq!(
        storage.put(&blocked_metadata, &mut Cursor::new(b"payload"), None),
        Err(StorageError::Unavailable)
    );
    assert!(!object_path(temporary.path(), &blocked_metadata).exists());
}

#[cfg(unix)]
#[test]
fn public_writes_reject_symlinked_and_unwritable_parents() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let external = temporary.path().join("external");
    std::fs::create_dir(&external).expect("external directory");
    symlink(&external, temporary.path().join("objects/linked")).expect("linked object parent");
    let linked = StorageKey::new("linked/object.bin").expect("key");
    assert_eq!(
        storage.put(&linked, &mut Cursor::new(b"payload"), None),
        Err(StorageError::InvalidInput)
    );

    let locked = temporary.path().join("objects/locked");
    std::fs::create_dir(&locked).expect("locked object parent");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555))
        .expect("lock object parent");
    let locked_key = StorageKey::new("locked/object.bin").expect("key");
    assert_eq!(
        storage.put(&locked_key, &mut Cursor::new(b"payload"), None),
        Err(StorageError::Unavailable)
    );
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755))
        .expect("unlock object parent");

    let linked_multipart_key = StorageKey::new("multipart-linked/object.bin").expect("key");
    let linked_upload = storage
        .begin_multipart(&linked_multipart_key, &expected_multipart())
        .expect("multipart upload");
    let linked_part = storage
        .put_part(&linked_upload, 1, &mut Cursor::new(b"part"))
        .expect("multipart part");
    symlink(&external, temporary.path().join("objects/multipart-linked"))
        .expect("linked multipart parent");
    assert_eq!(
        storage.complete_multipart(&linked_upload, &[linked_part]),
        Err(StorageError::InvalidInput)
    );
    storage
        .abort_multipart(&linked_upload)
        .expect("abort linked upload");

    let locked_multipart_key = StorageKey::new("multipart-locked/object.bin").expect("key");
    let locked_upload = storage
        .begin_multipart(&locked_multipart_key, &expected_multipart())
        .expect("multipart upload");
    let locked_part = storage
        .put_part(&locked_upload, 1, &mut Cursor::new(b"part"))
        .expect("multipart part");
    let locked_multipart = temporary.path().join("objects/multipart-locked");
    std::fs::create_dir(&locked_multipart).expect("locked multipart parent");
    std::fs::set_permissions(&locked_multipart, std::fs::Permissions::from_mode(0o555))
        .expect("lock multipart parent");
    assert_eq!(
        storage.complete_multipart(&locked_upload, &[locked_part]),
        Err(StorageError::Unavailable)
    );
    std::fs::set_permissions(&locked_multipart, std::fs::Permissions::from_mode(0o755))
        .expect("unlock multipart parent");
    storage
        .abort_multipart(&locked_upload)
        .expect("abort locked upload");
}

#[test]
fn multipart_rejects_invalid_ids_part_numbers_and_sequences() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let key = StorageKey::new("multipart/guarded.bin").expect("key");
    let upload = storage
        .begin_multipart(&key, &expected_multipart())
        .expect("upload");

    for number in [0, 10_001] {
        assert_eq!(
            storage.put_part(&upload, number, &mut Cursor::new(b"part")),
            Err(StorageError::InvalidInput)
        );
    }
    for id in ["", "bad/id", "a name", &"a".repeat(65)] {
        assert_eq!(
            storage.put_part(&MultipartId(id.to_owned()), 1, &mut Cursor::new(b"part")),
            Err(StorageError::InvalidInput)
        );
    }
    let missing_upload = MultipartId("missing-upload".to_owned());
    let missing_part = MultipartPart {
        number: 1,
        size: 1,
        checksum: ObjectChecksum::new("a".repeat(64)).expect("checksum"),
        provider_tag: None,
    };
    assert_eq!(
        storage.put_part(&missing_upload, 1, &mut Cursor::new(b"part")),
        Err(StorageError::NotFound)
    );
    assert_eq!(
        storage.complete_multipart(&missing_upload, &[missing_part]),
        Err(StorageError::NotFound)
    );
    assert_eq!(
        storage.abort_multipart(&missing_upload),
        Err(StorageError::NotFound)
    );
    assert_eq!(
        storage.complete_multipart(&upload, &[]),
        Err(StorageError::InvalidInput)
    );
    let skipped = [MultipartPart {
        number: 2,
        size: 1,
        checksum: ObjectChecksum::new("a".repeat(64)).expect("checksum"),
        provider_tag: None,
    }];
    assert_eq!(
        storage.complete_multipart(&upload, &skipped),
        Err(StorageError::InvalidInput)
    );
    storage.abort_multipart(&upload).expect("abort upload");
}

#[path = "filesystem_edges/multipart_integrity.rs"]
mod multipart_integrity;

#[test]
fn multipart_commit_rolls_back_bytes_on_metadata_conflict() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let key = StorageKey::new("multipart/conflict.bin").expect("key");
    let upload = storage
        .begin_multipart(&key, &expected_multipart())
        .expect("upload");
    let part = storage
        .put_part(&upload, 1, &mut Cursor::new(b"part"))
        .expect("part");
    let metadata = metadata_path(temporary.path(), &key);
    std::fs::create_dir_all(metadata.parent().expect("metadata parent"))
        .expect("metadata directory");
    std::fs::write(metadata, b"occupied").expect("metadata conflict fixture");

    assert_eq!(
        storage.complete_multipart(&upload, &[part]),
        Err(StorageError::Conflict)
    );
    assert!(!object_path(temporary.path(), &key).exists());
    storage.abort_multipart(&upload).expect("abort upload");
}

#[cfg(unix)]
#[test]
fn opening_a_symlink_root_fails_closed() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().expect("temporary directory");
    let actual = temporary.path().join("actual");
    let linked = temporary.path().join("linked");
    std::fs::create_dir(&actual).expect("actual directory");
    symlink(&actual, &linked).expect("root symlink");
    assert!(matches!(
        FilesystemStorage::open(&linked),
        Err(StorageError::InvalidInput)
    ));
}
