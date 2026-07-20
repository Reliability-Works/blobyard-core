use super::{Fixtures, storage_conformance_with};
use blobyard_contract::{
    ByteRange, MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, StorageError, StorageKey,
    StorageMetadata, StorageRead,
};
use std::io::Read;

struct UnusedStorage;

fn unavailable<T>() -> Result<T, StorageError> {
    Err(StorageError::Unavailable)
}

impl ObjectStorage for UnusedStorage {
    fn put(
        &self,
        _key: &StorageKey,
        _source: &mut dyn Read,
        _expected: Option<&ObjectChecksum>,
    ) -> Result<StorageMetadata, StorageError> {
        unavailable()
    }

    fn get(
        &self,
        _key: &StorageKey,
        _range: Option<ByteRange>,
    ) -> Result<StorageRead, StorageError> {
        unavailable()
    }

    fn head(&self, _key: &StorageKey) -> Result<StorageMetadata, StorageError> {
        unavailable()
    }

    fn delete(&self, _key: &StorageKey) -> Result<(), StorageError> {
        unavailable()
    }

    fn begin_multipart(
        &self,
        _key: &StorageKey,
        _expected: &StorageMetadata,
    ) -> Result<MultipartId, StorageError> {
        unavailable()
    }

    fn put_part(
        &self,
        _upload: &MultipartId,
        _number: u32,
        _source: &mut dyn Read,
    ) -> Result<MultipartPart, StorageError> {
        unavailable()
    }

    fn complete_multipart(
        &self,
        _upload: &MultipartId,
        _parts: &[MultipartPart],
    ) -> Result<StorageMetadata, StorageError> {
        unavailable()
    }

    fn abort_multipart(&self, _upload: &MultipartId) -> Result<(), StorageError> {
        unavailable()
    }
}

#[test]
fn conformance_rejects_each_invalid_fixture_value() {
    let mutations: [fn(&mut Fixtures); 7] = [
        |value: &mut Fixtures| value.hello_key = "../hello",
        |value: &mut Fixtures| value.hello_checksum = "invalid",
        |value: &mut Fixtures| {
            value.range_start = 4;
            value.range_end = 1;
        },
        |value: &mut Fixtures| value.bad_key = "../bad",
        |value: &mut Fixtures| value.empty_key = "../empty",
        |value: &mut Fixtures| value.multipart_key = "../multipart",
        |value: &mut Fixtures| value.abandoned_key = "../abandoned",
    ];
    for mutate in mutations {
        let mut fixtures = Fixtures::valid();
        mutate(&mut fixtures);
        assert!(storage_conformance_with(&UnusedStorage, &fixtures).is_err());
    }
}
