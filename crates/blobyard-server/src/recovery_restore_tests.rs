#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    RestoreFault, apply, cleanup, cleanup_after, copy_control_files, persist_restored_stage,
    require_empty_storage, restore_objects, seed_storage, set_fault, total_bytes, validate_backup,
    verify_hash,
};
use crate::recovery::{
    RecoveryError,
    io::{IoFault, with_fault},
    manifest::{BackupManifest, BackupObject},
    test_support,
};
use blobyard_contract::StorageError;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

fn create_backup() -> (tempfile::TempDir, std::path::PathBuf) {
    let source = test_support::installation();
    let parent = tempfile::tempdir().expect("backup parent");
    let backup = parent.path().join("backup");
    crate::backup_data_directory(
        source.path(),
        &backup,
        &crate::StorageConfiguration::Filesystem,
    )
    .expect("backup");
    (parent, backup)
}

fn read_json(backup: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(backup.join("manifest.json")).expect("manifest"))
        .expect("manifest json")
}

fn write_json(backup: &Path, value: &serde_json::Value) {
    fs::write(
        backup.join("manifest.json"),
        serde_json::to_vec_pretty(value).expect("manifest json"),
    )
    .expect("write manifest");
}

fn valid_manifest() -> BackupManifest {
    BackupManifest::new(
        blobyard_repository_sqlite::current_schema_version(),
        test_support::sha256(b"metadata"),
        test_support::sha256(b"runtime"),
        vec![BackupObject::new(
            test_support::KEY.to_owned(),
            test_support::CONTENT.len() as u64,
            test_support::sha256(test_support::CONTENT),
        )],
    )
}

#[test]
fn backup_validation_rejects_schema_hash_database_secret_and_symlink_tampering() {
    let (_parent, backup) = create_backup();
    assert!(validate_backup(&backup).is_ok());

    let original_manifest = read_json(&backup);
    let mut value = original_manifest.clone();
    value["metadataSchemaVersion"] = serde_json::json!(0);
    write_json(&backup, &value);
    assert_eq!(validate_backup(&backup), Err(RecoveryError::SchemaTooOld));

    let mut value = original_manifest.clone();
    value["metadataSchemaVersion"] =
        serde_json::json!(blobyard_repository_sqlite::current_schema_version() + 1);
    write_json(&backup, &value);
    assert_eq!(validate_backup(&backup), Err(RecoveryError::SchemaTooNew));

    let mut value = original_manifest.clone();
    value["metadataSchemaVersion"] =
        serde_json::json!(blobyard_repository_sqlite::current_schema_version() - 1);
    write_json(&backup, &value);
    assert_eq!(validate_backup(&backup), Err(RecoveryError::InvalidBackup));

    write_json(&backup, &original_manifest);
    let original_metadata = fs::read(backup.join("metadata.sqlite3")).expect("metadata");
    fs::write(backup.join("metadata.sqlite3"), b"tampered").expect("tamper metadata");
    assert_eq!(validate_backup(&backup), Err(RecoveryError::Integrity));

    let mut value = original_manifest.clone();
    value["metadataSha256"] = serde_json::json!(test_support::sha256(b"tampered"));
    write_json(&backup, &value);
    assert_eq!(validate_backup(&backup), Err(RecoveryError::Database));

    fs::write(backup.join("metadata.sqlite3"), &original_metadata).expect("restore metadata");
    write_json(&backup, &original_manifest);
    fs::write(backup.join("runtime.secret"), b"tampered").expect("tamper secret");
    assert_eq!(validate_backup(&backup), Err(RecoveryError::Integrity));

    let mut value = original_manifest.clone();
    value["runtimeSecretSha256"] = serde_json::json!(test_support::sha256(b""));
    write_json(&backup, &value);
    fs::write(backup.join("runtime.secret"), b"").expect("empty secret");
    assert_eq!(validate_backup(&backup), Err(RecoveryError::InvalidBackup));

    let mut value = original_manifest;
    value["runtimeSecretSha256"] = serde_json::json!(test_support::sha256(&[0xff]));
    write_json(&backup, &value);
    fs::write(backup.join("runtime.secret"), [0xff]).expect("invalid utf8 secret");
    assert_eq!(validate_backup(&backup), Err(RecoveryError::InvalidBackup));

    fs::write(backup.join("runtime.secret"), b"secret").expect("replacement secret");
    fs::remove_file(backup.join("metadata.sqlite3")).expect("remove metadata");
    symlink("runtime.secret", backup.join("metadata.sqlite3")).expect("metadata symlink");
    assert_eq!(validate_backup(&backup), Err(RecoveryError::InvalidBackup));
}

#[test]
fn control_file_copy_is_streamed_verified_nonempty_and_private() {
    let root = tempfile::tempdir().expect("root");
    let backup = root.path().join("backup");
    let stage = root.path().join("stage");
    fs::create_dir(&backup).expect("backup");
    fs::create_dir(&stage).expect("stage");
    fs::write(backup.join("metadata.sqlite3"), b"metadata").expect("metadata");
    fs::write(backup.join("runtime.secret"), b"runtime").expect("runtime");
    let manifest = valid_manifest();
    copy_control_files(&backup, &stage, &manifest).expect("copy control files");
    assert_eq!(
        fs::read(stage.join("metadata.sqlite3")).expect("metadata"),
        b"metadata"
    );

    let empty = root.path().join("empty");
    fs::create_dir(&empty).expect("empty backup");
    fs::write(empty.join("metadata.sqlite3"), b"").expect("empty metadata");
    fs::write(empty.join("runtime.secret"), b"runtime").expect("runtime");
    let empty_manifest = BackupManifest::new(
        blobyard_repository_sqlite::current_schema_version(),
        test_support::sha256(b""),
        test_support::sha256(b"runtime"),
        Vec::new(),
    );
    assert_eq!(
        copy_control_files(&empty, &root.path().join("empty-stage"), &empty_manifest),
        Err(RecoveryError::Integrity)
    );

    assert_eq!(
        verify_hash(
            &backup,
            Path::new("metadata.sqlite3"),
            &test_support::sha256(b"metadata")
        ),
        Ok(())
    );
    assert_eq!(
        verify_hash(
            &backup,
            Path::new("metadata.sqlite3"),
            &test_support::sha256(b"other")
        ),
        Err(RecoveryError::Integrity)
    );
    assert_eq!(
        with_fault(IoFault::HashRead, || {
            verify_hash(
                &backup,
                Path::new("metadata.sqlite3"),
                &test_support::sha256(b"metadata"),
            )
        }),
        Err(RecoveryError::Storage)
    );

    let blocked_stage = root.path().join("blocked-stage");
    fs::create_dir(&blocked_stage).expect("blocked stage");
    fs::write(blocked_stage.join("metadata.sqlite3"), b"occupied").expect("occupied metadata");
    assert_eq!(
        copy_control_files(&backup, &blocked_stage, &manifest),
        Err(RecoveryError::Persistence)
    );
}

#[path = "recovery_restore_object_tests.rs"]
mod object_tests;

#[path = "recovery_restore_state_tests.rs"]
mod state_tests;
