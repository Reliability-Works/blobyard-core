use crate::S3Storage;
use blobyard_contract::{ObjectChecksum, StorageError, StorageMetadata};
use std::collections::HashMap;

const CHECKSUM_KEY: &str = "blobyard-sha256";
const SIZE_KEY: &str = "blobyard-size";

impl S3Storage {
    pub(crate) fn encode_metadata(metadata: &StorageMetadata) -> HashMap<String, String> {
        HashMap::from([
            (
                CHECKSUM_KEY.to_owned(),
                metadata.checksum.as_str().to_owned(),
            ),
            (SIZE_KEY.to_owned(), metadata.size.to_string()),
        ])
    }

    pub(crate) fn decode_metadata(
        provider_size: Option<i64>,
        values: Option<&HashMap<String, String>>,
    ) -> Result<StorageMetadata, StorageError> {
        let values = values.ok_or(StorageError::IntegrityMismatch)?;
        let declared_size = values
            .get(SIZE_KEY)
            .ok_or(StorageError::IntegrityMismatch)?
            .parse::<u64>()
            .map_err(|_error| StorageError::IntegrityMismatch)?;
        let actual_size = provider_size
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(StorageError::IntegrityMismatch)?;
        if declared_size != actual_size {
            return Err(StorageError::IntegrityMismatch);
        }
        let checksum = values
            .get(CHECKSUM_KEY)
            .ok_or(StorageError::IntegrityMismatch)
            .and_then(|value| {
                ObjectChecksum::new(value.clone()).map_err(|_error| StorageError::IntegrityMismatch)
            })?;
        Ok(StorageMetadata {
            size: actual_size,
            checksum,
        })
    }
}
