use blobyard_contract::{
    ByteRange, ObjectChecksum, ObjectStorage, StorageError, StorageKey, StorageMetadata,
};
use std::io::{Cursor, Read};

/// Runs the deterministic byte-storage contract against one empty adapter.
///
/// # Errors
///
/// Returns the first contract failure reported by the adapter.
pub fn storage_conformance(storage: &dyn ObjectStorage) -> Result<(), StorageError> {
    storage_conformance_with(storage, &Fixtures::valid())
}

fn storage_conformance_with(
    storage: &dyn ObjectStorage,
    fixtures: &Fixtures,
) -> Result<(), StorageError> {
    fixtures
        .validate()
        .and_then(|fixtures| storage_conformance_validated(storage, &fixtures))
}

fn storage_conformance_validated(
    storage: &dyn ObjectStorage,
    fixtures: &ValidatedFixtures,
) -> Result<(), StorageError> {
    let key = &fixtures.hello_key;
    let checksum = &fixtures.hello_checksum;
    let metadata = storage.put(key, &mut Cursor::new(b"hello"), Some(checksum))?;
    if metadata.size != 5 || &metadata.checksum != checksum || storage.head(key)? != metadata {
        return Err(StorageError::Unavailable);
    }
    assert_bytes(storage.get(key, None)?, b"hello")?;
    assert_bytes(storage.get(key, Some(fixtures.range))?, b"ell")?;
    if storage.put(key, &mut Cursor::new(b"again"), None) != Err(StorageError::Conflict) {
        return Err(StorageError::Unavailable);
    }
    let bad_key = &fixtures.bad_key;
    if storage.put(bad_key, &mut Cursor::new(b"bad"), Some(checksum))
        != Err(StorageError::IntegrityMismatch)
        || storage.head(bad_key) != Err(StorageError::NotFound)
    {
        return Err(StorageError::Unavailable);
    }
    empty_object_conformance(storage, fixtures)?;
    multipart_conformance(storage, fixtures)?;
    storage.delete(key)?;
    if storage.head(key) != Err(StorageError::NotFound) {
        return Err(StorageError::Unavailable);
    }
    Ok(())
}

fn empty_object_conformance(
    storage: &dyn ObjectStorage,
    fixtures: &ValidatedFixtures,
) -> Result<(), StorageError> {
    let key = &fixtures.empty_key;
    let metadata = storage.put(key, &mut Cursor::new([]), None)?;
    if metadata.size != 0 {
        return Err(StorageError::Unavailable);
    }
    assert_bytes(storage.get(key, None)?, b"")?;
    storage.delete(key)
}

fn multipart_conformance(
    storage: &dyn ObjectStorage,
    fixtures: &ValidatedFixtures,
) -> Result<(), StorageError> {
    let expected = StorageMetadata {
        size: 6,
        checksum: ObjectChecksum::from_sha256_digest([
            0xbe, 0xf5, 0x7e, 0xc7, 0xf5, 0x3a, 0x6d, 0x40, 0xbe, 0xb6, 0x40, 0xa7, 0x80, 0xa6,
            0x39, 0xc8, 0x3b, 0xc2, 0x9a, 0xc8, 0xa9, 0x81, 0x6f, 0x1f, 0xc6, 0xc5, 0xc6, 0xdc,
            0xd9, 0x3c, 0x47, 0x21,
        ]),
    };
    let upload = storage.begin_multipart(&fixtures.multipart_key, &expected)?;
    let first = storage.put_part(&upload, 1, &mut Cursor::new(b"abc"))?;
    let second = storage.put_part(&upload, 2, &mut Cursor::new(b"def"))?;
    let metadata = storage.complete_multipart(&upload, &[first, second])?;
    if metadata.size != 6 {
        return Err(StorageError::Unavailable);
    }
    assert_bytes(storage.get(&fixtures.multipart_key, None)?, b"abcdef")?;
    storage.delete(&fixtures.multipart_key)?;
    let abandoned_expected = StorageMetadata {
        size: 6,
        checksum: ObjectChecksum::from_sha256_digest([
            0xfe, 0xbe, 0x1d, 0x74, 0x1b, 0x49, 0xe5, 0xa9, 0xc3, 0x15, 0x26, 0x72, 0x8d, 0x8c,
            0x51, 0x34, 0xa8, 0x03, 0xad, 0xfc, 0x4c, 0x04, 0xc4, 0xf0, 0x52, 0x67, 0x37, 0x22,
            0xed, 0x85, 0x59, 0x7e,
        ]),
    };
    let abandoned = storage.begin_multipart(&fixtures.abandoned_key, &abandoned_expected)?;
    storage.put_part(&abandoned, 1, &mut Cursor::new(b"unused"))?;
    storage.abort_multipart(&abandoned)?;
    if storage.abort_multipart(&abandoned) != Err(StorageError::NotFound) {
        return Err(StorageError::Unavailable);
    }
    Ok(())
}

fn assert_bytes(
    mut read: blobyard_contract::StorageRead,
    expected: &[u8],
) -> Result<(), StorageError> {
    let mut bytes = Vec::new();
    read.reader
        .read_to_end(&mut bytes)
        .map_err(|_error| StorageError::Unavailable)?;
    if bytes == expected {
        Ok(())
    } else {
        Err(StorageError::Unavailable)
    }
}

const fn hello_checksum() -> &'static str {
    "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
}

struct Fixtures {
    hello_key: &'static str,
    hello_checksum: &'static str,
    range_start: u64,
    range_end: u64,
    bad_key: &'static str,
    empty_key: &'static str,
    multipart_key: &'static str,
    abandoned_key: &'static str,
}

impl Fixtures {
    const fn valid() -> Self {
        Self {
            hello_key: "fixtures/hello.txt",
            hello_checksum: hello_checksum(),
            range_start: 1,
            range_end: 4,
            bad_key: "fixtures/bad.txt",
            empty_key: "fixtures/empty.bin",
            multipart_key: "fixtures/multipart.bin",
            abandoned_key: "fixtures/abandoned",
        }
    }

    fn validate(&self) -> Result<ValidatedFixtures, StorageError> {
        Ok(ValidatedFixtures {
            hello_key: StorageKey::new(self.hello_key)?,
            hello_checksum: ObjectChecksum::new(self.hello_checksum)?,
            range: ByteRange::new(self.range_start, self.range_end)?,
            bad_key: StorageKey::new(self.bad_key)?,
            empty_key: StorageKey::new(self.empty_key)?,
            multipart_key: StorageKey::new(self.multipart_key)?,
            abandoned_key: StorageKey::new(self.abandoned_key)?,
        })
    }
}

struct ValidatedFixtures {
    hello_key: StorageKey,
    hello_checksum: ObjectChecksum,
    range: ByteRange,
    bad_key: StorageKey,
    empty_key: StorageKey,
    multipart_key: StorageKey,
    abandoned_key: StorageKey,
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
