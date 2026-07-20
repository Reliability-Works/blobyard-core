//! Real `MinIO` acceptance for the public Blob Yard S3 adapter contract.

use blobyard_contract::{
    ByteRange, ObjectChecksum, ObjectStorage, StorageError, StorageKey, StorageMetadata,
};
use blobyard_core::SecretString;
use blobyard_storage_s3::{S3Credentials, S3Storage, S3StorageConfig};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::io::{Cursor, Read};

type AcceptanceResult<T = ()> = Result<T, Box<dyn Error>>;

fn main() -> AcceptanceResult {
    let settings = Settings::from_environment()?;
    let temporary = tempfile::tempdir()?;
    let config = settings.config(temporary.path())?;
    let storage = S3Storage::open(&config)?;

    println!("Checking initial missing object");
    prove_missing_object(&storage)?;
    println!("Checking object round trip");
    prove_object_round_trip(&storage)?;
    println!("Checking adapter restart");
    prove_restart(&config)?;
    println!("Checking multipart completion");
    prove_multipart(&storage)?;
    println!("Checking multipart abort");
    prove_abort(&storage)?;
    println!("Checking object cleanup");
    prove_cleanup(&storage)?;

    println!("MinIO acceptance passed");
    Ok(())
}

fn prove_missing_object(storage: &S3Storage) -> AcceptanceResult {
    let key = StorageKey::new("objects/initially-missing.bin")?;
    require(
        storage.head(&key) == Err(StorageError::NotFound),
        "initial missing object",
    )
}

struct Settings {
    endpoint: String,
    bucket: String,
    access_key: String,
    secret_key: String,
}

impl Settings {
    fn from_environment() -> AcceptanceResult<Self> {
        Ok(Self {
            endpoint: required_environment("BLOBYARD_MINIO_ENDPOINT")?,
            bucket: required_environment("BLOBYARD_MINIO_BUCKET")?,
            access_key: required_environment("BLOBYARD_MINIO_ACCESS_KEY")?,
            secret_key: required_environment("BLOBYARD_MINIO_SECRET_KEY")?,
        })
    }

    fn config(&self, staging: &std::path::Path) -> AcceptanceResult<S3StorageConfig> {
        let credentials = S3Credentials::new(
            SecretString::new(self.access_key.clone())?,
            SecretString::new(self.secret_key.clone())?,
            None,
        );
        Ok(S3StorageConfig::new(
            &self.endpoint,
            "us-east-1",
            &self.bucket,
            credentials,
            staging.to_path_buf(),
        )?
        .with_prefix(Some("acceptance"))?
        .with_force_path_style(true))
    }
}

fn prove_object_round_trip(storage: &S3Storage) -> AcceptanceResult {
    let key = StorageKey::new("objects/round-trip.bin")?;
    let bytes = b"blobyard-minio-round-trip";
    let expected = checksum(bytes);
    println!("  putting object");
    let metadata = storage.put(&key, &mut Cursor::new(bytes), Some(&expected))?;
    require(metadata == expected_metadata(bytes), "object put metadata")?;
    println!("  reading object metadata");
    require(storage.head(&key)? == metadata, "object head metadata")?;

    println!("  downloading full object");
    let (full, full_metadata, full_range) = read(storage, &key, None)?;
    require(full == bytes, "full object bytes")?;
    require(full_metadata == metadata, "full object metadata")?;
    require(
        full_range == ByteRange::new(0, bytes.len() as u64)?,
        "full object range",
    )?;

    let requested = ByteRange::new(4, 11)?;
    println!("  downloading object range");
    let (ranged, ranged_metadata, actual_range) = read(storage, &key, Some(requested))?;
    require(ranged == bytes[4..11], "ranged object bytes")?;
    require(ranged_metadata == metadata, "ranged object metadata")?;
    require(actual_range == requested, "ranged object range")?;

    println!("  checking conditional conflict");
    require(
        storage.put(&key, &mut Cursor::new(bytes), Some(&expected)) == Err(StorageError::Conflict),
        "conditional object conflict",
    )?;
    println!("  checking the client after conflict");
    prove_missing_object(storage)
}

fn prove_restart(config: &S3StorageConfig) -> AcceptanceResult {
    let restarted = S3Storage::open(config)?;
    let key = StorageKey::new("objects/round-trip.bin")?;
    let (bytes, metadata, _) = read(&restarted, &key, None)?;
    require(bytes == b"blobyard-minio-round-trip", "restart bytes")?;
    require(metadata == restarted.head(&key)?, "restart metadata")
}

fn prove_multipart(storage: &S3Storage) -> AcceptanceResult {
    let key = StorageKey::new("objects/multipart.bin")?;
    let first = vec![b'a'; 5 * 1024 * 1024];
    let second = b"multipart-tail".to_vec();
    let mut complete = first.clone();
    complete.extend_from_slice(&second);
    let expected = expected_metadata(&complete);
    println!("  checking missing multipart target");
    require(
        storage.head(&key) == Err(StorageError::NotFound),
        "multipart target starts absent",
    )?;
    println!("  beginning multipart upload");
    let upload = storage.begin_multipart(&key, &expected)?;
    println!("  uploading first multipart part");
    let part_one = storage.put_part(&upload, 1, &mut Cursor::new(&first))?;
    println!("  uploading second multipart part");
    let part_two = storage.put_part(&upload, 2, &mut Cursor::new(&second))?;
    require(part_one.provider_tag.is_some(), "first provider part tag")?;
    require(part_two.provider_tag.is_some(), "second provider part tag")?;
    println!("  completing multipart upload");
    require(
        storage.complete_multipart(&upload, &[part_one, part_two])? == expected,
        "multipart completion metadata",
    )?;
    println!("  downloading completed multipart object");
    let (stored, metadata, _) = read(storage, &key, None)?;
    require(stored == complete, "multipart bytes")?;
    require(metadata == expected, "multipart read metadata")
}

fn prove_abort(storage: &S3Storage) -> AcceptanceResult {
    let key = StorageKey::new("objects/aborted.bin")?;
    let bytes = b"never-committed";
    let expected = expected_metadata(bytes);
    let upload = storage.begin_multipart(&key, &expected)?;
    let part = storage.put_part(&upload, 1, &mut Cursor::new(bytes))?;
    require(part.provider_tag.is_some(), "abort provider part tag")?;
    storage.abort_multipart(&upload)?;
    require(
        storage.abort_multipart(&upload) == Err(StorageError::NotFound),
        "second abort reports missing provider state",
    )?;
    require(
        storage.head(&key) == Err(StorageError::NotFound),
        "aborted object remains absent",
    )
}

fn prove_cleanup(storage: &S3Storage) -> AcceptanceResult {
    for value in ["objects/round-trip.bin", "objects/multipart.bin"] {
        let key = StorageKey::new(value)?;
        storage.delete(&key)?;
        require(
            storage.head(&key) == Err(StorageError::NotFound),
            "deleted object remains absent",
        )?;
    }
    Ok(())
}

fn read(
    storage: &S3Storage,
    key: &StorageKey,
    range: Option<ByteRange>,
) -> AcceptanceResult<(Vec<u8>, StorageMetadata, ByteRange)> {
    let mut value = storage.get(key, range)?;
    let mut bytes = Vec::new();
    value.reader.read_to_end(&mut bytes)?;
    Ok((bytes, value.metadata, value.range))
}

fn expected_metadata(bytes: &[u8]) -> StorageMetadata {
    StorageMetadata {
        size: bytes.len() as u64,
        checksum: checksum(bytes),
    }
}

fn checksum(bytes: &[u8]) -> ObjectChecksum {
    ObjectChecksum::from_sha256_digest(Sha256::digest(bytes).into())
}

fn required_environment(name: &str) -> AcceptanceResult<String> {
    std::env::var(name)
        .map_err(|_error| std::io::Error::other(format!("required environment is missing: {name}")))
        .map_err(Into::into)
}

fn require(condition: bool, label: &'static str) -> AcceptanceResult {
    if condition {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("acceptance assertion failed: {label}")).into())
    }
}
