#![allow(clippy::expect_used, reason = "test synchronization must fail loudly")]

use blobyard_contract::{
    ByteRange, MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, StorageError, StorageKey,
    StorageMetadata, StorageRead,
};
use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Result-corruption adapters for conformance assertions.
pub mod corrupting;
pub(crate) use corrupting::{Corrupting, Corruption};

pub(crate) struct Faulting<'a, T> {
    inner: &'a T,
    remaining: AtomicUsize,
}

impl<'a, T> Faulting<'a, T> {
    pub(crate) const fn new(inner: &'a T, failure_index: usize) -> Self {
        Self {
            inner,
            remaining: AtomicUsize::new(failure_index),
        }
    }

    fn check(&self) -> Result<(), StorageError> {
        self.remaining
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                remaining.checked_sub(1)
            })
            .map(|_previous| ())
            .map_err(|_current| StorageError::Unavailable)
    }
}

impl<T: ObjectStorage> ObjectStorage for Faulting<'_, T> {
    fn put(
        &self,
        key: &StorageKey,
        source: &mut dyn Read,
        expected: Option<&ObjectChecksum>,
    ) -> Result<StorageMetadata, StorageError> {
        self.check()?;
        self.inner.put(key, source, expected)
    }

    fn get(&self, key: &StorageKey, range: Option<ByteRange>) -> Result<StorageRead, StorageError> {
        self.check()?;
        self.inner.get(key, range)
    }

    fn head(&self, key: &StorageKey) -> Result<StorageMetadata, StorageError> {
        self.check()?;
        self.inner.head(key)
    }

    fn delete(&self, key: &StorageKey) -> Result<(), StorageError> {
        self.check()?;
        self.inner.delete(key)
    }

    fn begin_multipart(
        &self,
        key: &StorageKey,
        expected: &StorageMetadata,
    ) -> Result<MultipartId, StorageError> {
        self.check()?;
        self.inner.begin_multipart(key, expected)
    }

    fn put_part(
        &self,
        upload: &MultipartId,
        number: u32,
        source: &mut dyn Read,
    ) -> Result<MultipartPart, StorageError> {
        self.check()?;
        self.inner.put_part(upload, number, source)
    }

    fn complete_multipart(
        &self,
        upload: &MultipartId,
        parts: &[MultipartPart],
    ) -> Result<StorageMetadata, StorageError> {
        self.check()?;
        self.inner.complete_multipart(upload, parts)
    }

    fn abort_multipart(&self, upload: &MultipartId) -> Result<(), StorageError> {
        self.check()?;
        self.inner.abort_multipart(upload)
    }
}
