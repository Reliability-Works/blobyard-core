use super::S3Storage;
use crate::client::S3Client;
use blobyard_contract::{
    MultipartId, MultipartPart, ObjectStorage, StorageError, StorageKey, StorageMetadata,
};
use std::io::Read;

struct MultipartTarget {
    key: StorageKey,
    upload_id: String,
    client: S3Client,
    provider_key: String,
}

impl S3Storage {
    pub(crate) fn create_multipart(
        &self,
        key: &StorageKey,
        expected: &StorageMetadata,
    ) -> Result<MultipartId, StorageError> {
        match self.head(key) {
            Ok(_metadata) => return Err(StorageError::Conflict),
            Err(StorageError::NotFound) => {}
            Err(error) => return Err(error),
        }
        let client = self.client.clone();
        let provider_key = self.provider_key(key);
        let values = Self::encode_metadata(expected);
        let upload_id = self
            .runtime
            .run(async move { client.create_multipart(&provider_key, &values).await })?;
        super::MultipartLocator::encode(key, &upload_id)
    }

    pub(crate) fn upload_part(
        &self,
        upload: &MultipartId,
        number: u32,
        source: &mut dyn Read,
    ) -> Result<MultipartPart, StorageError> {
        provider_part_number(number)?;
        let locator = super::MultipartLocator::decode(upload)?;
        let staged = Self::stage_upload(&self.staging_directory, source)?;
        let client = self.client.clone();
        let body_builder = self.body_builder;
        let provider_key = self.provider_key(&locator.key);
        let provider_upload_id = locator.upload_id;
        let metadata = staged.metadata;
        let upload_size = metadata.size;
        let upload_checksum = metadata.checksum.as_str().to_owned();
        let provider_tag = self.runtime.run(async move {
            let temporary = staged.temporary;
            let body = body_builder(temporary.path().to_path_buf()).await?;
            let provider_tag = client
                .upload_part(
                    &provider_key,
                    &provider_upload_id,
                    number,
                    upload_size,
                    &upload_checksum,
                    body,
                )
                .await?;
            if valid_provider_tag(&provider_tag) {
                Ok(provider_tag)
            } else {
                Err(StorageError::Unavailable)
            }
        })?;
        Ok(MultipartPart {
            number,
            size: metadata.size,
            checksum: metadata.checksum,
            provider_tag: Some(provider_tag),
        })
    }

    pub(crate) fn commit_multipart(
        &self,
        upload: &MultipartId,
        parts: &[MultipartPart],
    ) -> Result<StorageMetadata, StorageError> {
        let completed = completed_parts(parts)?;
        let MultipartTarget {
            key,
            upload_id,
            client,
            provider_key,
        } = self.multipart_target(upload)?;
        self.runtime.run(async move {
            client
                .complete_multipart(&provider_key, &upload_id, &completed)
                .await
        })?;
        self.verify_completed_object(&key)
    }

    pub(crate) fn cancel_multipart(&self, upload: &MultipartId) -> Result<(), StorageError> {
        let target = self.multipart_target(upload)?;
        self.runtime.run(async move {
            target
                .client
                .list_parts(&target.provider_key, &target.upload_id)
                .await?;
            target
                .client
                .abort_multipart(&target.provider_key, &target.upload_id)
                .await
        })
    }

    fn multipart_target(&self, upload: &MultipartId) -> Result<MultipartTarget, StorageError> {
        let locator = super::MultipartLocator::decode(upload)?;
        Ok(MultipartTarget {
            provider_key: self.provider_key(&locator.key),
            key: locator.key,
            upload_id: locator.upload_id,
            client: self.client.clone(),
        })
    }

    fn verify_completed_object(&self, key: &StorageKey) -> Result<StorageMetadata, StorageError> {
        let result = self
            .head(key)
            .and_then(|metadata| self.get(key, None).map(|_read| metadata));
        match result {
            Ok(metadata) => Ok(metadata),
            Err(error) => match self.remove_object(key) {
                Ok(()) | Err(StorageError::NotFound) => Err(error),
                Err(_cleanup_error) => Err(StorageError::Unavailable),
            },
        }
    }
}

fn completed_parts(parts: &[MultipartPart]) -> Result<Vec<MultipartPart>, StorageError> {
    if parts.is_empty() || parts.len() > 10_000 {
        return Err(StorageError::InvalidInput);
    }
    parts
        .iter()
        .zip(1_u32..=10_000)
        .map(|(part, expected)| {
            let tag = part
                .provider_tag
                .as_deref()
                .filter(|value| valid_provider_tag(value))
                .ok_or(StorageError::InvalidInput)?;
            if part.number != expected {
                return Err(StorageError::InvalidInput);
            }
            let mut completed = part.clone();
            completed.provider_tag = Some(tag.to_owned());
            Ok(completed)
        })
        .collect()
}

fn provider_part_number(number: u32) -> Result<u32, StorageError> {
    if (1..=10_000).contains(&number) {
        Ok(number)
    } else {
        Err(StorageError::InvalidInput)
    }
}

fn valid_provider_tag(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}
