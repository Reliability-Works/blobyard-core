#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use blobyard_contract::{
    ByteRange, MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, StorageError, StorageKey,
    StorageMetadata, StorageRead,
};
use std::io::{Cursor, Error, Read};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Corruption {
    MetadataSize,
    MetadataChecksum,
    MetadataHead,
    FullReadBytes,
    FullReadFailure,
    RangeReadBytes,
    RepeatPutError,
    IntegrityPutError,
    IntegrityHeadError,
    DeletedHeadError,
    EmptySize,
    EmptyReadBytes,
    MultipartSize,
    MultipartReadBytes,
    RepeatedAbortError,
}

pub(crate) struct Corrupting<'a, T> {
    inner: &'a T,
    corruption: Corruption,
}

impl<'a, T> Corrupting<'a, T> {
    pub(crate) const fn new(inner: &'a T, corruption: Corruption) -> Self {
        Self { inner, corruption }
    }
}

#[derive(Debug)]
struct FailingRead;

impl Read for FailingRead {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Err(Error::other("fixture read failure"))
    }
}

impl<T: ObjectStorage> ObjectStorage for Corrupting<'_, T> {
    fn put(
        &self,
        key: &StorageKey,
        source: &mut dyn Read,
        expected: Option<&ObjectChecksum>,
    ) -> Result<StorageMetadata, StorageError> {
        let result = self.inner.put(key, source, expected);
        match (self.corruption, key.as_str(), result) {
            (Corruption::MetadataSize, "fixtures/hello.txt", Ok(mut value)) => {
                value.size = 6;
                Ok(value)
            }
            (Corruption::MetadataChecksum, "fixtures/hello.txt", Ok(mut value)) => {
                value.checksum = ObjectChecksum::new("a".repeat(64)).expect("checksum");
                Ok(value)
            }
            (Corruption::RepeatPutError, "fixtures/hello.txt", Err(StorageError::Conflict))
            | (Corruption::IntegrityPutError, "fixtures/bad.txt", Err(_)) => {
                Err(StorageError::InvalidInput)
            }
            (Corruption::EmptySize, "fixtures/empty.bin", Ok(mut value)) => {
                value.size = 1;
                Ok(value)
            }
            (_, _, result) => result,
        }
    }

    fn get(&self, key: &StorageKey, range: Option<ByteRange>) -> Result<StorageRead, StorageError> {
        self.inner.get(key, range).map(|mut value| {
            match (self.corruption, key.as_str(), range) {
                (Corruption::FullReadBytes, "fixtures/hello.txt", None)
                | (Corruption::MultipartReadBytes, "fixtures/multipart.bin", None)
                | (Corruption::EmptyReadBytes, "fixtures/empty.bin", None) => {
                    value.reader = Box::new(Cursor::new(b"wrong".to_vec()));
                }
                (Corruption::FullReadFailure, "fixtures/hello.txt", None) => {
                    value.reader = Box::new(FailingRead);
                }
                (Corruption::RangeReadBytes, "fixtures/hello.txt", Some(_)) => {
                    value.reader = Box::new(Cursor::new(b"bad".to_vec()));
                }
                _ => {}
            }
            value
        })
    }

    fn head(&self, key: &StorageKey) -> Result<StorageMetadata, StorageError> {
        let result = self.inner.head(key);
        match (self.corruption, key.as_str(), result) {
            (Corruption::MetadataHead, "fixtures/hello.txt", Ok(mut value)) => {
                value.size += 1;
                Ok(value)
            }
            (Corruption::IntegrityHeadError, "fixtures/bad.txt", Err(_))
            | (Corruption::DeletedHeadError, "fixtures/hello.txt", Err(_)) => {
                Err(StorageError::InvalidInput)
            }
            (_, _, result) => result,
        }
    }

    fn delete(&self, key: &StorageKey) -> Result<(), StorageError> {
        self.inner.delete(key)
    }

    blobyard_testkit::impl_forwarding_multipart_start!();

    fn complete_multipart(
        &self,
        upload: &MultipartId,
        parts: &[MultipartPart],
    ) -> Result<StorageMetadata, StorageError> {
        self.inner
            .complete_multipart(upload, parts)
            .map(|mut value| {
                if matches!(self.corruption, Corruption::MultipartSize) {
                    value.size += 1;
                }
                value
            })
    }

    fn abort_multipart(&self, upload: &MultipartId) -> Result<(), StorageError> {
        let result = self.inner.abort_multipart(upload);
        if matches!(self.corruption, Corruption::RepeatedAbortError)
            && result == Err(StorageError::NotFound)
        {
            Err(StorageError::InvalidInput)
        } else {
            result
        }
    }
}
