#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{collect_entries, collect_entry, collect_files, key_from_path};
use crate::FilesystemStorage;
use blobyard_contract::{ObjectStorage, ObjectStorageInventory, StorageError, StorageKey};
use std::io::Cursor;

#[test]
fn inventory_lists_only_committed_objects_in_stable_key_order() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    for key in ["z-last", "nested/b-first", "nested/a-first"] {
        storage
            .put(
                &StorageKey::new(key).expect("key"),
                &mut Cursor::new(key.as_bytes()),
                None,
            )
            .expect("put object");
    }
    let upload = storage
        .begin_multipart(
            &StorageKey::new("hidden/multipart").expect("key"),
            &blobyard_contract::StorageMetadata {
                size: 1,
                checksum: blobyard_contract::ObjectChecksum::from_sha256_digest([0; 32]),
            },
        )
        .expect("multipart");

    let keys = storage.list_object_keys().expect("inventory");
    assert_eq!(
        keys.iter().map(StorageKey::as_str).collect::<Vec<_>>(),
        ["nested/a-first", "nested/b-first", "z-last"]
    );
    storage.abort_multipart(&upload).expect("abort multipart");
}

#[test]
fn inventory_fails_closed_for_unsafe_physical_entries() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let objects = temporary.path().join("objects");
    std::fs::write(objects.join("valid"), b"bytes").expect("object");

    let outside = tempfile::tempdir().expect("outside");
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;

        std::os::unix::fs::symlink(outside.path(), objects.join("link")).expect("symlink");
        assert_eq!(storage.list_object_keys(), Err(StorageError::InvalidInput));
        std::fs::remove_file(objects.join("link")).expect("remove link");

        let socket = std::os::unix::net::UnixListener::bind(objects.join("socket"))
            .expect("Unix socket fixture");
        assert_eq!(storage.list_object_keys(), Err(StorageError::InvalidInput));
        drop(socket);
        std::fs::remove_file(objects.join("socket")).expect("remove socket");

        let non_utf8 = objects.join(std::ffi::OsString::from_vec(vec![0xff]));
        assert_eq!(
            key_from_path(&objects, &non_utf8),
            Err(StorageError::InvalidInput)
        );
    }

    assert_eq!(
        key_from_path(&objects, outside.path()),
        Err(StorageError::InvalidInput)
    );
    let mut files = Vec::new();
    assert_eq!(
        collect_files(&objects.join("missing"), &mut files),
        Err(StorageError::Unavailable)
    );
    let mut failing_entries = vec![Err::<std::fs::DirEntry, _>(std::io::Error::other(
        "entry failure",
    ))]
    .into_iter();
    assert_eq!(
        collect_entries(&mut failing_entries).err(),
        Some(StorageError::Unavailable)
    );
    assert_eq!(
        collect_entry(
            objects.join("unreadable"),
            Err(std::io::Error::other("file type failure")),
            &mut files,
            &mut Vec::new(),
        ),
        Err(StorageError::Unavailable)
    );
}
