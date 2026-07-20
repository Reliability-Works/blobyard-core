#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use blobyard_contract::{
    ByteRange, MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, ObjectStorageInventory,
    StorageError, StorageKey, StorageMetadata, StorageRead,
};
use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::storage_get_macro::storage_read_methods;
use crate::storage_multipart_macro::storage_multipart_error;

#[derive(Clone, Copy, Debug)]
pub(super) enum PutBehavior {
    Mismatch,
    Integrity,
    Unavailable,
    SuccessThenMismatch,
    SuccessThenIntegrity,
    SuccessThenUnavailable,
}

pub(super) struct FaultStorage {
    pub(super) put: PutBehavior,
    pub(super) delete_fails: bool,
    pub(super) list_fails: bool,
    puts: AtomicUsize,
}

impl FaultStorage {
    pub(super) const fn new(put: PutBehavior) -> Self {
        Self {
            put,
            delete_fails: false,
            list_fails: false,
            puts: AtomicUsize::new(0),
        }
    }
}

impl ObjectStorage for FaultStorage {
    fn put(
        &self,
        _key: &StorageKey,
        _source: &mut dyn Read,
        expected: Option<&ObjectChecksum>,
    ) -> Result<StorageMetadata, StorageError> {
        let call = self.puts.fetch_add(1, Ordering::Relaxed);
        if call == 0
            && matches!(
                self.put,
                PutBehavior::SuccessThenMismatch
                    | PutBehavior::SuccessThenIntegrity
                    | PutBehavior::SuccessThenUnavailable
            )
        {
            return Ok(StorageMetadata {
                size: 3,
                checksum: expected.cloned().ok_or(StorageError::Unavailable)?,
            });
        }
        match self.put {
            PutBehavior::Mismatch | PutBehavior::SuccessThenMismatch => Ok(StorageMetadata {
                size: 0,
                checksum: ObjectChecksum::new("0".repeat(64)).expect("checksum"),
            }),
            PutBehavior::Integrity | PutBehavior::SuccessThenIntegrity => {
                Err(StorageError::IntegrityMismatch)
            }
            PutBehavior::Unavailable | PutBehavior::SuccessThenUnavailable => {
                Err(StorageError::Unavailable)
            }
        }
    }

    storage_read_methods!(
        self,
        StorageError::Unavailable,
        Err(StorageError::Unavailable)
    );

    fn delete(&self, _key: &StorageKey) -> Result<(), StorageError> {
        if self.delete_fails {
            Err(StorageError::Unavailable)
        } else {
            Ok(())
        }
    }

    storage_multipart_error!(StorageError::Unavailable);
}

impl ObjectStorageInventory for FaultStorage {
    fn list_object_keys(&self) -> Result<Vec<StorageKey>, StorageError> {
        if self.list_fails {
            Err(StorageError::Unavailable)
        } else {
            Ok(Vec::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn unused_storage_contract_paths_fail_deterministically() {
        let storage = FaultStorage::new(PutBehavior::Unavailable);
        let key = StorageKey::new("fixture/object").expect("key");
        let checksum = ObjectChecksum::new("0".repeat(64)).expect("checksum");
        let metadata = StorageMetadata { size: 0, checksum };
        let upload = MultipartId("upload_fixture".to_owned());
        let part = MultipartPart {
            number: 1,
            size: 0,
            checksum: ObjectChecksum::new("0".repeat(64)).expect("part checksum"),
            provider_tag: Some("etag_fixture".to_owned()),
        };

        assert_eq!(
            storage.get(&key, None).err(),
            Some(StorageError::Unavailable)
        );
        assert_eq!(storage.head(&key), Err(StorageError::Unavailable));
        assert_eq!(
            storage.begin_multipart(&key, &metadata),
            Err(StorageError::Unavailable)
        );
        assert_eq!(
            storage.put_part(&upload, 1, &mut Cursor::new([])),
            Err(StorageError::Unavailable)
        );
        assert_eq!(
            storage.complete_multipart(&upload, &[part]),
            Err(StorageError::Unavailable)
        );
        assert_eq!(
            storage.abort_multipart(&upload),
            Err(StorageError::Unavailable)
        );
        assert!(storage.delete(&key).is_ok());
        assert_eq!(storage.list_object_keys(), Ok(Vec::new()));
    }
}
