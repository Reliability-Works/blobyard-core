use crate::MultipartLocator;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use blobyard_contract::{MultipartId, StorageError, StorageKey};

const PREFIX: &str = "s3v1.";
const MAX_PROVIDER_ID_BYTES: usize = 2_048;
const MAX_ENCODED_BYTES: usize = 4_096;

impl MultipartLocator {
    pub(crate) fn encode(key: &StorageKey, upload_id: &str) -> Result<MultipartId, StorageError> {
        validate_provider_id(upload_id)?;
        let key_bytes = key.as_str().as_bytes();
        encode_key_length(key_bytes.len()).map(|key_length| {
            let mut wire = Vec::with_capacity(2 + key_bytes.len() + upload_id.len());
            wire.extend_from_slice(&key_length.to_be_bytes());
            wire.extend_from_slice(key_bytes);
            wire.extend_from_slice(upload_id.as_bytes());
            MultipartId(format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(wire)))
        })
    }

    pub(crate) fn decode(value: &MultipartId) -> Result<Self, StorageError> {
        let encoded = value
            .0
            .strip_prefix(PREFIX)
            .filter(|value| !value.is_empty() && value.len() <= MAX_ENCODED_BYTES)
            .ok_or(StorageError::InvalidInput)?;
        let wire = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_error| StorageError::InvalidInput)?;
        let (key_length, payload) = decode_key_length(&wire)?;
        let key_end = usize::from(key_length);
        let (key, upload_id) = payload
            .split_at_checked(key_end)
            .ok_or(StorageError::InvalidInput)?;
        let key = std::str::from_utf8(key).map_err(|_error| StorageError::InvalidInput)?;
        let upload_id =
            std::str::from_utf8(upload_id).map_err(|_error| StorageError::InvalidInput)?;
        validate_provider_id(upload_id)?;
        Ok(Self {
            key: StorageKey::new(key.to_owned())?,
            upload_id: upload_id.to_owned(),
        })
    }
}

fn encode_key_length(length: usize) -> Result<u16, StorageError> {
    u16::try_from(length).map_err(|_error| StorageError::InvalidInput)
}

fn decode_key_length(wire: &[u8]) -> Result<(u16, &[u8]), StorageError> {
    let bytes: [u8; 2] = wire
        .get(..2)
        .and_then(|value| value.try_into().ok())
        .ok_or(StorageError::InvalidInput)?;
    Ok((u16::from_be_bytes(bytes), &wire[2..]))
}

fn validate_provider_id(value: &str) -> Result<(), StorageError> {
    if value.is_empty()
        || value.len() > MAX_PROVIDER_ID_BYTES
        || value.chars().any(char::is_control)
    {
        Err(StorageError::InvalidInput)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::encode_key_length;
    use blobyard_contract::StorageError;

    #[test]
    fn impossible_key_lengths_still_fail_closed() {
        assert_eq!(
            encode_key_length(usize::from(u16::MAX) + 1),
            Err(StorageError::InvalidInput)
        );
    }
}
