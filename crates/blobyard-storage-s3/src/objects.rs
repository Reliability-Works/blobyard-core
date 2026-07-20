use super::S3Storage;
use blobyard_contract::{
    ByteRange, ObjectChecksum, ObjectStorage, StorageError, StorageKey, StorageMetadata,
    StorageRead,
};
use std::io::Read;
use std::path::Path;
use tempfile::NamedTempFile;

impl S3Storage {
    pub(crate) fn put_object(
        &self,
        key: &StorageKey,
        source: &mut dyn Read,
        expected: Option<&ObjectChecksum>,
    ) -> Result<StorageMetadata, StorageError> {
        let staged = Self::stage_upload(&self.staging_directory, source)?;
        if expected.is_some_and(|checksum| checksum != &staged.metadata.checksum) {
            return Err(StorageError::IntegrityMismatch);
        }
        let client = self.client.clone();
        let body_builder = self.body_builder;
        let provider_key = self.provider_key(key);
        let values = Self::encode_metadata(&staged.metadata);
        let metadata = staged.metadata.clone();
        self.runtime.run(async move {
            let temporary = staged.temporary;
            let body = body_builder(temporary.path().to_path_buf()).await?;
            client
                .put_object(
                    &provider_key,
                    &values,
                    metadata.size,
                    metadata.checksum.as_str(),
                    body,
                )
                .await?;
            Ok(metadata)
        })
    }

    pub(crate) fn get_object(
        &self,
        key: &StorageKey,
        requested: Option<ByteRange>,
    ) -> Result<StorageRead, StorageError> {
        let metadata = self.head(key)?;
        let range = validate_range(requested, metadata.size)?;
        if range.start == range.end {
            return Self::empty_download(&self.staging_directory).map(|reader| StorageRead {
                reader: Box::new(reader),
                metadata,
                range,
            });
        }
        let temporary = NamedTempFile::new_in(&self.staging_directory)
            .map_err(|_error| StorageError::Unavailable)?;
        let count = self.download(key, requested.map(|_| range), &temporary)?;
        verify_download(temporary.path(), count, range, &metadata)?;
        super::StagedRead::open(temporary).map(|reader| StorageRead {
            reader: Box::new(reader),
            metadata,
            range,
        })
    }

    pub(crate) fn head_object(&self, key: &StorageKey) -> Result<StorageMetadata, StorageError> {
        let client = self.client.clone();
        let provider_key = self.provider_key(key);
        self.runtime.run(async move {
            let output = client.head_object(&provider_key).await?;
            Self::decode_metadata(output.content_length, Some(&output.metadata))
        })
    }

    pub(crate) fn delete_object(&self, key: &StorageKey) -> Result<(), StorageError> {
        self.head(key)?;
        self.remove_object(key)
    }

    pub(crate) fn remove_object(&self, key: &StorageKey) -> Result<(), StorageError> {
        let client = self.client.clone();
        let provider_key = self.provider_key(key);
        self.runtime
            .run(async move { client.delete_object(&provider_key).await })
    }

    fn download(
        &self,
        key: &StorageKey,
        range: Option<ByteRange>,
        temporary: &NamedTempFile,
    ) -> Result<u64, StorageError> {
        let client = self.client.clone();
        let provider_key = self.provider_key(key);
        let path = temporary.path().to_path_buf();
        self.runtime.run(async move {
            let range = range.map(|value| format!("bytes={}-{}", value.start, value.end - 1));
            client
                .get_object(&provider_key, range.as_deref(), path)
                .await
        })
    }
}

fn verify_download(
    path: &Path,
    count: u64,
    range: ByteRange,
    metadata: &StorageMetadata,
) -> Result<(), StorageError> {
    if count != range.end - range.start {
        return Err(StorageError::IntegrityMismatch);
    }
    if range.start == 0 && range.end == metadata.size {
        return S3Storage::hash_path(path).and_then(|actual| {
            if actual == *metadata {
                Ok(())
            } else {
                Err(StorageError::IntegrityMismatch)
            }
        });
    }
    Ok(())
}

fn validate_range(requested: Option<ByteRange>, size: u64) -> Result<ByteRange, StorageError> {
    let range = requested.unwrap_or(ByteRange {
        start: 0,
        end: size,
    });
    if range.start <= range.end && range.end <= size {
        Ok(range)
    } else {
        Err(StorageError::InvalidInput)
    }
}

#[cfg(test)]
mod tests {
    use super::verify_download;
    use blobyard_contract::{ByteRange, ObjectChecksum, StorageError, StorageMetadata};

    #[test]
    fn full_download_hash_read_failure_is_stable() {
        let metadata = StorageMetadata {
            size: 5,
            checksum: ObjectChecksum::from_sha256_digest([0_u8; 32]),
        };
        assert_eq!(
            verify_download(
                std::path::Path::new("\0"),
                5,
                ByteRange { start: 0, end: 5 },
                &metadata,
            ),
            Err(StorageError::Unavailable)
        );
    }
}
