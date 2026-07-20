use blobyard_contract::{ObjectChecksum, StorageError, StorageMetadata};
use std::collections::HashMap;

fn checksum() -> Result<ObjectChecksum, StorageError> {
    ObjectChecksum::new("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
}

#[test]
fn metadata_round_trips_exact_size_and_checksum() -> Result<(), StorageError> {
    let expected = StorageMetadata {
        size: 5,
        checksum: checksum()?,
    };
    let encoded = crate::S3Storage::encode_metadata(&expected);
    assert_eq!(
        crate::S3Storage::decode_metadata(Some(5), Some(&encoded))?,
        expected
    );
    Ok(())
}

#[test]
fn metadata_rejects_missing_malformed_and_mismatched_values() -> Result<(), StorageError> {
    let valid = crate::S3Storage::encode_metadata(&StorageMetadata {
        size: 5,
        checksum: checksum()?,
    });
    let mut cases = vec![None, Some(HashMap::new())];
    let mut missing_checksum = valid.clone();
    missing_checksum.remove("blobyard-sha256");
    cases.push(Some(missing_checksum));
    let mut missing_size = valid.clone();
    missing_size.remove("blobyard-size");
    cases.push(Some(missing_size));
    let mut invalid_size = valid.clone();
    invalid_size.insert("blobyard-size".to_owned(), "invalid".to_owned());
    cases.push(Some(invalid_size));
    let mut invalid_checksum = valid.clone();
    invalid_checksum.insert("blobyard-sha256".to_owned(), "invalid".to_owned());
    cases.push(Some(invalid_checksum));
    for values in cases {
        assert_eq!(
            crate::S3Storage::decode_metadata(Some(5), values.as_ref()),
            Err(StorageError::IntegrityMismatch)
        );
    }
    for provider_size in [None, Some(-1), Some(4)] {
        assert_eq!(
            crate::S3Storage::decode_metadata(provider_size, Some(&valid)),
            Err(StorageError::IntegrityMismatch)
        );
    }
    Ok(())
}
