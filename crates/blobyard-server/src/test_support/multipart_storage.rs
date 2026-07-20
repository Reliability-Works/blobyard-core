#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use blobyard_contract::{
    ByteRange, MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, StorageError, StorageKey,
    StorageMetadata, StorageRead,
};
use std::io::Read;

use crate::storage_get_macro::storage_read_methods;
use crate::storage_part_macro::storage_part_error;
use crate::storage_put_macro::storage_put_error;

#[derive(Clone)]
pub struct MultipartStorage {
    pub head: Result<StorageMetadata, StorageError>,
    pub delete: Result<(), StorageError>,
    pub begin: Result<MultipartId, StorageError>,
    pub complete: Result<StorageMetadata, StorageError>,
    pub abort: Result<(), StorageError>,
}

impl MultipartStorage {
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            head: Err(StorageError::Unavailable),
            delete: Err(StorageError::Unavailable),
            begin: Err(StorageError::Unavailable),
            complete: Err(StorageError::Unavailable),
            abort: Err(StorageError::Unavailable),
        }
    }
}

impl ObjectStorage for MultipartStorage {
    storage_put_error!(StorageError::Unavailable);
    storage_read_methods!(self, StorageError::Unavailable, self.head.clone());

    fn delete(&self, _key: &StorageKey) -> Result<(), StorageError> {
        self.delete
    }

    fn begin_multipart(
        &self,
        _key: &StorageKey,
        _expected: &StorageMetadata,
    ) -> Result<MultipartId, StorageError> {
        self.begin.clone()
    }

    storage_part_error!(StorageError::Unavailable);

    fn complete_multipart(
        &self,
        _upload: &MultipartId,
        _parts: &[MultipartPart],
    ) -> Result<StorageMetadata, StorageError> {
        self.complete.clone()
    }

    fn abort_multipart(&self, _upload: &MultipartId) -> Result<(), StorageError> {
        self.abort
    }
}

#[test]
fn unavailable_fixture_exposes_every_storage_boundary() {
    let storage = MultipartStorage::unavailable();
    let key = StorageKey::new("valid/key").expect("key");
    let checksum = ObjectChecksum::new("a".repeat(64)).expect("checksum");
    let mut source = std::io::Cursor::new(Vec::<u8>::new());
    assert_eq!(
        storage.put(&key, &mut source, Some(&checksum)),
        Err(StorageError::Unavailable)
    );
    assert_eq!(
        storage.get(&key, None).err().expect("read failure"),
        StorageError::Unavailable
    );
    assert_eq!(storage.head(&key), Err(StorageError::Unavailable));
    assert_eq!(storage.delete(&key), Err(StorageError::Unavailable));
    crate::test_support::assert_multipart_unavailable(&storage);
}
