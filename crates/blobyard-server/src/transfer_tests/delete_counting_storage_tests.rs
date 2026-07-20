use blobyard_contract::{
    ByteRange, MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, StorageError, StorageKey,
    StorageMetadata, StorageRead,
};
use std::io::Read;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct DeleteCountingStorage {
    inner: Arc<dyn ObjectStorage>,
    calls: Arc<AtomicUsize>,
}

pub(super) fn new(
    inner: Arc<dyn ObjectStorage>,
    calls: Arc<AtomicUsize>,
) -> Arc<dyn ObjectStorage> {
    Arc::new(DeleteCountingStorage { inner, calls })
}

impl ObjectStorage for DeleteCountingStorage {
    fn put(
        &self,
        key: &StorageKey,
        source: &mut dyn Read,
        expected: Option<&ObjectChecksum>,
    ) -> Result<StorageMetadata, StorageError> {
        self.inner.put(key, source, expected)
    }

    fn get(&self, key: &StorageKey, range: Option<ByteRange>) -> Result<StorageRead, StorageError> {
        self.inner.get(key, range)
    }

    fn head(&self, key: &StorageKey) -> Result<StorageMetadata, StorageError> {
        self.inner.head(key)
    }

    fn delete(&self, key: &StorageKey) -> Result<(), StorageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.delete(key)
    }

    blobyard_testkit::impl_forwarding_multipart_start!();

    fn complete_multipart(
        &self,
        upload: &MultipartId,
        parts: &[MultipartPart],
    ) -> Result<StorageMetadata, StorageError> {
        self.inner.complete_multipart(upload, parts)
    }

    fn abort_multipart(&self, upload: &MultipartId) -> Result<(), StorageError> {
        self.inner.abort_multipart(upload)
    }
}
