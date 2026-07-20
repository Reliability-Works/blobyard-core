#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{BackupManifest, BackupObject, ManifestEncoder};
use crate::recovery::{RecoveryError, io};

const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn manifest(objects: Vec<BackupObject>) -> BackupManifest {
    BackupManifest::new(15, HASH_A.to_owned(), HASH_B.to_owned(), objects)
}

fn object(key: &str, checksum: &str) -> BackupObject {
    BackupObject::new(key.to_owned(), 4, checksum.to_owned())
}

#[test]
fn manifest_constructor_sorts_and_round_trips_strict_json() {
    let root = tempfile::tempdir().expect("root");
    let expected = manifest(vec![
        object("objects/z", HASH_B),
        object("objects/a", HASH_A),
    ]);
    assert_eq!(expected.objects[0].storage_key, "objects/a");
    expected.write(root.path()).expect("write manifest");
    assert_eq!(BackupManifest::read(root.path()), Ok(expected));

    let unknown = br#"{
        "formatVersion":1,
        "coreVersion":"1",
        "metadataSchemaVersion":15,
        "metadataSha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "runtimeSecretSha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "objects":[],
        "unexpected":true
    }"#;
    let other = tempfile::tempdir().expect("other");
    io::write_private_file(&other.path().join("manifest.json"), unknown).expect("unknown manifest");
    assert_eq!(
        BackupManifest::read(other.path()),
        Err(RecoveryError::InvalidBackup)
    );
}

#[test]
fn manifest_validation_rejects_every_unsupported_or_ambiguous_shape() {
    let valid = manifest(vec![object("objects/a", HASH_A)]);

    let mut candidate = valid.clone();
    candidate.format_version = 2;
    assert_eq!(candidate.validate(), Err(RecoveryError::InvalidBackup));

    let mut candidate = valid.clone();
    candidate.core_version.clear();
    assert_eq!(candidate.validate(), Err(RecoveryError::InvalidBackup));

    let mut candidate = valid.clone();
    candidate.metadata_sha256 = "bad".to_owned();
    assert_eq!(candidate.validate(), Err(RecoveryError::InvalidBackup));

    let mut candidate = valid.clone();
    candidate.runtime_secret_sha256 = "bad".to_owned();
    assert_eq!(candidate.validate(), Err(RecoveryError::InvalidBackup));

    let mut candidate = valid.clone();
    candidate.objects[0].storage_key = "../escape".to_owned();
    assert_eq!(candidate.validate(), Err(RecoveryError::InvalidBackup));

    let mut candidate = valid;
    candidate.objects[0].checksum = "bad".to_owned();
    assert_eq!(candidate.validate(), Err(RecoveryError::InvalidBackup));

    let duplicate = manifest(vec![
        object("objects/a", HASH_A),
        object("objects/a", HASH_B),
    ]);
    assert_eq!(duplicate.validate(), Err(RecoveryError::InvalidBackup));

    let mut unsorted = manifest(vec![
        object("objects/a", HASH_A),
        object("objects/z", HASH_B),
    ]);
    unsorted.objects.swap(0, 1);
    assert_eq!(unsorted.validate(), Err(RecoveryError::InvalidBackup));
}

#[test]
fn manifest_read_rejects_missing_and_malformed_files() {
    let missing = tempfile::tempdir().expect("missing");
    assert_eq!(
        BackupManifest::read(missing.path()),
        Err(RecoveryError::InvalidBackup)
    );

    let malformed = tempfile::tempdir().expect("malformed");
    io::write_private_file(&malformed.path().join("manifest.json"), b"not-json")
        .expect("malformed manifest");
    assert_eq!(
        BackupManifest::read(malformed.path()),
        Err(RecoveryError::InvalidBackup)
    );

    let invalid = tempfile::tempdir().expect("invalid");
    let mut value = serde_json::to_value(manifest(Vec::new())).expect("manifest value");
    value["formatVersion"] = serde_json::json!(2);
    io::write_private_file(
        &invalid.path().join("manifest.json"),
        &serde_json::to_vec(&value).expect("invalid manifest"),
    )
    .expect("invalid manifest file");
    assert_eq!(
        BackupManifest::read(invalid.path()),
        Err(RecoveryError::InvalidBackup)
    );

    assert_eq!(
        manifest(Vec::new()).write_with(invalid.path(), &FailingEncoder),
        Err(RecoveryError::Persistence)
    );
}

#[derive(Debug)]
struct FailingEncoder;

impl ManifestEncoder for FailingEncoder {
    fn encode(&self, _manifest: &BackupManifest) -> Result<Vec<u8>, ()> {
        Err(())
    }
}
