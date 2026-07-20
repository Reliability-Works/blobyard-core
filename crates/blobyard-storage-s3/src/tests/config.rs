use crate::{S3Credentials, S3Storage, S3StorageConfig};
use blobyard_contract::{StorageError, StorageKey};
use blobyard_core::{GeneratedSecretKind, SecretString};

fn credentials() -> S3Credentials {
    S3Credentials::new(
        SecretString::from_generated_entropy(GeneratedSecretKind::AccessToken, [1; 32]),
        SecretString::from_generated_entropy(GeneratedSecretKind::RuntimeSecret, [2; 32]),
        Some(SecretString::from_generated_entropy(
            GeneratedSecretKind::MachineToken,
            [3; 32],
        )),
    )
}

#[test]
fn configuration_validates_connection_prefix_and_redacts_credentials() -> Result<(), StorageError> {
    let temporary = tempfile::tempdir().map_err(|_error| StorageError::Unavailable)?;
    let config = S3StorageConfig::new(
        "https://objects.example.com",
        "auto",
        "bucket",
        credentials(),
        temporary.path().join("stage"),
    )?
    .with_prefix(Some("tenant/core"))?
    .with_force_path_style(true);
    let debug = format!("{config:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("bya_"));
    let storage = S3Storage::open(&config)?;
    assert_eq!(
        storage.provider_key(&StorageKey::new("objects/file")?),
        "tenant/core/objects/file"
    );
    assert!(!format!("{storage:?}").contains("bya_"));

    let http = S3StorageConfig::new(
        "http://localhost:9000",
        "us-east-1",
        "bucket",
        credentials(),
        temporary.path().join("http-stage"),
    )?;
    let http_storage = S3Storage::open(&http)?;
    assert_eq!(
        http_storage.provider_key(&StorageKey::new("objects/http")?),
        "objects/http"
    );
    Ok(())
}

#[test]
fn configuration_rejects_every_unsafe_boundary() {
    let temporary = std::path::PathBuf::from("stage");
    for endpoint in [
        "invalid",
        "ftp://example.com",
        "https://user@example.com",
        "https://user:pass@example.com",
        "https://example.com/path",
        "https://example.com/?query=yes",
        "https://example.com/#fragment",
    ] {
        assert_eq!(
            S3StorageConfig::new(endpoint, "auto", "bucket", credentials(), temporary.clone())
                .map(|_config| ()),
            Err(StorageError::InvalidInput)
        );
    }
    for (region, bucket) in [
        ("", "bucket"),
        ("bad\nregion", "bucket"),
        ("r", ""),
        ("r", "bad\nbucket"),
    ] {
        assert_eq!(
            S3StorageConfig::new(
                "https://example.com",
                region,
                bucket,
                credentials(),
                temporary.clone(),
            )
            .map(|_config| ()),
            Err(StorageError::InvalidInput)
        );
    }
    assert_eq!(
        S3StorageConfig::new(
            "https://example.com",
            "auto",
            "bucket",
            credentials(),
            std::path::PathBuf::new(),
        )
        .map(|_config| ()),
        Err(StorageError::InvalidInput)
    );
}

#[test]
fn prefix_and_staging_fail_closed() -> Result<(), StorageError> {
    let config = S3StorageConfig::new(
        "https://example.com",
        "auto",
        "bucket",
        credentials(),
        std::path::PathBuf::from("stage"),
    )?;
    assert_eq!(
        config.with_prefix(Some("../escape")).map(|_value| ()),
        Err(StorageError::InvalidInput)
    );
    let temporary = tempfile::tempdir().map_err(|_error| StorageError::Unavailable)?;
    let file = temporary.path().join("not-a-directory");
    std::fs::write(&file, b"file").map_err(|_error| StorageError::Unavailable)?;
    let invalid =
        S3StorageConfig::new("https://example.com", "auto", "bucket", credentials(), file)?;
    assert_eq!(
        S3Storage::open(&invalid).map(|_storage| ()),
        Err(StorageError::Unavailable)
    );

    let https = S3StorageConfig::new(
        "https://example.com",
        "auto",
        "bucket",
        credentials(),
        temporary.path().join("tls-stage"),
    )?;
    assert_eq!(
        https
            .client_from_transport(Err(StorageError::Unavailable))
            .err(),
        Some(StorageError::Unavailable)
    );

    let client = https.client();
    assert_eq!(
        S3Storage::from_config_results(&https, Err(StorageError::Unavailable), client,).err(),
        Some(StorageError::Unavailable)
    );
    assert_eq!(
        S3Storage::from_config_results(
            &https,
            crate::RuntimeBridge::start(),
            Err(StorageError::Unavailable),
        )
        .err(),
        Some(StorageError::Unavailable)
    );
    Ok(())
}
