#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{S3RuntimeConfiguration, StorageConfiguration};
use blobyard_core::{GeneratedSecretKind, SecretString};

#[test]
fn filesystem_is_the_redaction_safe_default() {
    assert!(matches!(
        StorageConfiguration::default(),
        StorageConfiguration::Filesystem
    ));
    assert_eq!(
        format!("{:?}", StorageConfiguration::default()),
        "Filesystem"
    );
}

#[test]
fn s3_configuration_redacts_credentials_and_validates_on_open() {
    let access = SecretString::from_generated_entropy(GeneratedSecretKind::AccessToken, [1; 32]);
    let secret = SecretString::from_generated_entropy(GeneratedSecretKind::RuntimeSecret, [2; 32]);
    let token = SecretString::from_generated_entropy(GeneratedSecretKind::MachineToken, [3; 32]);
    let access_value = access.expose_secret().to_owned();
    let config = S3RuntimeConfiguration::new(
        "invalid".to_owned(),
        "region".to_owned(),
        "bucket".to_owned(),
        access,
        secret,
        Some(token),
    )
    .with_prefix(Some("tenant/core".to_owned()))
    .with_force_path_style(true);
    let debug = format!("{config:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains(&access_value));
    let temporary = tempfile::tempdir().expect("temporary directory");
    assert_eq!(
        config.open(temporary.path()).err(),
        Some(crate::ServerError::Storage)
    );
}

#[test]
fn valid_s3_configuration_opens_through_runtime_selection() {
    let access = SecretString::from_generated_entropy(GeneratedSecretKind::AccessToken, [4; 32]);
    let secret = SecretString::from_generated_entropy(GeneratedSecretKind::RuntimeSecret, [5; 32]);
    let config = S3RuntimeConfiguration::new(
        "http://127.0.0.1:9000".to_owned(),
        "us-east-1".to_owned(),
        "blobyard".to_owned(),
        access,
        secret,
        None,
    )
    .with_prefix(Some("tenant/core".to_owned()))
    .with_force_path_style(true);
    let temporary = tempfile::tempdir().expect("temporary directory");

    let _storage = StorageConfiguration::S3(config)
        .open(temporary.path())
        .expect("S3 runtime storage");
}
