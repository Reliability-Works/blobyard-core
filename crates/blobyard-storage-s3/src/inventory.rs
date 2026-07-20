use crate::S3Storage;
use blobyard_contract::{ObjectStorageInventory, StorageError, StorageKey};

impl ObjectStorageInventory for S3Storage {
    fn list_object_keys(&self) -> Result<Vec<StorageKey>, StorageError> {
        let client = self.client.clone();
        let prefix = self.prefix.clone();
        self.runtime
            .run(async move { list_all(client, prefix).await })
    }
}

async fn list_all(
    client: crate::client::S3Client,
    prefix: Option<String>,
) -> Result<Vec<StorageKey>, StorageError> {
    let provider_prefix = prefix.as_ref().map(|value| format!("{value}/"));
    let mut continuation = None;
    let mut keys = Vec::new();
    loop {
        let output = client
            .list_objects(provider_prefix.as_deref(), continuation.as_deref())
            .await?;
        for provider_key in output.keys {
            keys.push(portable_key(&provider_key, provider_prefix.as_deref())?);
        }
        if !output.is_truncated {
            break;
        }
        let next = output
            .next_continuation_token
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or(StorageError::Unavailable)?
            .to_owned();
        if continuation.as_deref() == Some(next.as_str()) {
            return Err(StorageError::Unavailable);
        }
        continuation = Some(next);
    }
    keys.sort();
    if keys.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(StorageError::Unavailable);
    }
    Ok(keys)
}

fn portable_key(value: &str, prefix: Option<&str>) -> Result<StorageKey, StorageError> {
    let value = prefix.map_or(Some(value), |prefix| value.strip_prefix(prefix));
    StorageKey::new(value.ok_or(StorageError::InvalidInput)?.to_owned())
}
