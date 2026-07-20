#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Filesystem storage conformance and path safety guards.

use blobyard_contract::{ObjectStorage, StorageError, StorageKey};
use blobyard_storage_filesystem::FilesystemStorage;

#[test]
fn filesystem_satisfies_the_storage_contract() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    blobyard_testkit::storage_conformance(&storage).expect("conformance");
}

#[test]
fn filesystem_rejects_traversal_and_reports_missing_bytes() {
    assert_eq!(
        StorageKey::new("../outside"),
        Err(StorageError::InvalidInput)
    );
    let temporary = tempfile::tempdir().expect("temporary directory");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    let key = StorageKey::new("missing/object").expect("key");
    assert_eq!(storage.head(&key), Err(StorageError::NotFound));
}
