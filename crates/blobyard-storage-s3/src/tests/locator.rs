use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use blobyard_contract::{MultipartId, StorageError, StorageKey};

#[test]
fn locator_round_trips_without_exposing_provider_fields() -> Result<(), StorageError> {
    let key = StorageKey::new("objects/build.zip")?;
    let encoded = crate::MultipartLocator::encode(&key, "provider-upload")?;
    assert!(encoded.0.starts_with("s3v1."));
    assert!(!encoded.0.contains("provider-upload"));
    let decoded = crate::MultipartLocator::decode(&encoded)?;
    assert_eq!(decoded.key, key);
    assert_eq!(decoded.upload_id, "provider-upload");
    Ok(())
}

#[test]
fn locator_rejects_invalid_provider_ids_and_wire_shapes() -> Result<(), StorageError> {
    let key = StorageKey::new("object")?;
    for provider in [String::new(), "x".repeat(2_049), "bad\nid".to_owned()] {
        assert_eq!(
            crate::MultipartLocator::encode(&key, &provider),
            Err(StorageError::InvalidInput)
        );
    }
    for value in [
        "wrong.AA".to_owned(),
        "s3v1.".to_owned(),
        format!("s3v1.{}", "A".repeat(4_097)),
        "s3v1.invalid*".to_owned(),
        format!("s3v1.{}", URL_SAFE_NO_PAD.encode([0_u8])),
        format!("s3v1.{}", URL_SAFE_NO_PAD.encode([0_u8, 5, b'a'])),
        format!("s3v1.{}", URL_SAFE_NO_PAD.encode([0_u8, 0, b'x'])),
        format!("s3v1.{}", URL_SAFE_NO_PAD.encode([0_u8, 1, 0xff, b'x'])),
        format!("s3v1.{}", URL_SAFE_NO_PAD.encode([0_u8, 1, b'a', 0xff])),
        format!("s3v1.{}", URL_SAFE_NO_PAD.encode([0_u8, 1, b'a'])),
        format!("s3v1.{}", URL_SAFE_NO_PAD.encode([0_u8, 1, b'a', b'\n'])),
        format!(
            "s3v1.{}",
            URL_SAFE_NO_PAD.encode([0_u8, 2, b'.', b'.', b'x'])
        ),
    ] {
        assert!(crate::MultipartLocator::decode(&MultipartId(value)).is_err());
    }
    Ok(())
}
