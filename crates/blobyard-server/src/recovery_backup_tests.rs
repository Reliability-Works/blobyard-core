#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{BackupFault, add_bytes, copy_object, map_storage, read_runtime_secret, set_fault};
use crate::recovery::{RecoveryError, map_repository, test_support};
use blobyard_contract::{
    ObjectSource, ObjectVersionRecord, RepositoryError, StorageError, UploadState,
};
use std::fs;

fn record() -> ObjectVersionRecord {
    ObjectVersionRecord {
        id: "version".to_owned(),
        project_id: "project".to_owned(),
        object_path: "build.bin".to_owned(),
        version: 1,
        storage_key: test_support::KEY.to_owned(),
        state: UploadState::Complete,
        size: Some(test_support::CONTENT.len() as u64),
        checksum: Some(test_support::sha256(test_support::CONTENT)),
        created_at_ms: 1,
        source: ObjectSource::Cli,
        git_repository: None,
        git_commit: None,
        git_branch: None,
    }
}

fn assert_copy_mode(mode: test_support::GetMode, expected: RecoveryError) {
    let root = tempfile::tempdir().expect("root");
    let storage =
        test_support::ScriptedStorage::with_object(test_support::CONTENT).with_get_mode(mode);
    assert_eq!(copy_object(root.path(), &storage, &record()), Err(expected));
}

#[test]
fn object_copy_requires_valid_complete_metadata_and_exact_provider_bytes() {
    let root = tempfile::tempdir().expect("root");
    let storage = test_support::ScriptedStorage::with_object(test_support::CONTENT);
    let copied = copy_object(root.path(), &storage, &record()).expect("copy");
    assert_eq!(copied.storage_key, test_support::KEY);
    assert_eq!(copied.size, test_support::CONTENT.len() as u64);
    assert_eq!(
        fs::read(root.path().join("objects").join(test_support::KEY)).expect("copied bytes"),
        test_support::CONTENT
    );

    let mut invalid = record();
    invalid.storage_key = "../escape".to_owned();
    assert_eq!(
        copy_object(root.path(), &storage, &invalid),
        Err(RecoveryError::Integrity)
    );
    let mut missing_size = record();
    missing_size.size = None;
    assert_eq!(
        copy_object(root.path(), &storage, &missing_size),
        Err(RecoveryError::Integrity)
    );
    let mut missing_checksum = record();
    missing_checksum.checksum = None;
    assert_eq!(
        copy_object(root.path(), &storage, &missing_checksum),
        Err(RecoveryError::Integrity)
    );
    let mut invalid_checksum = record();
    invalid_checksum.checksum = Some("bad".to_owned());
    assert_eq!(
        copy_object(root.path(), &storage, &invalid_checksum),
        Err(RecoveryError::Integrity)
    );
}

#[test]
fn object_copy_classifies_provider_metadata_reader_and_persistence_failures() {
    for error in [
        StorageError::NotFound,
        StorageError::Conflict,
        StorageError::InvalidInput,
        StorageError::Unavailable,
    ] {
        assert_copy_mode(test_support::GetMode::Error(error), RecoveryError::Storage);
    }

    assert_copy_mode(
        test_support::GetMode::Error(StorageError::IntegrityMismatch),
        RecoveryError::Integrity,
    );
    assert_copy_mode(
        test_support::GetMode::MetadataMismatch,
        RecoveryError::Integrity,
    );
    assert_copy_mode(
        test_support::GetMode::ReaderMismatch,
        RecoveryError::Integrity,
    );
    assert_copy_mode(test_support::GetMode::ReaderFailure, RecoveryError::Storage);

    let root = tempfile::tempdir().expect("root");
    fs::create_dir(root.path().join("objects")).expect("objects");
    fs::write(root.path().join("objects/objects"), b"blocking file").expect("blocking file");
    let storage = test_support::ScriptedStorage::with_object(test_support::CONTENT);
    assert_eq!(
        copy_object(root.path(), &storage, &record()),
        Err(RecoveryError::Persistence)
    );
}

#[test]
fn runtime_secret_rejects_missing_empty_and_non_utf8_values() {
    let source = test_support::installation();
    assert!(
        !read_runtime_secret(source.path())
            .expect("runtime secret")
            .is_empty()
    );

    fs::write(source.path().join("runtime.secret"), [0xff]).expect("invalid utf8");
    assert_eq!(
        read_runtime_secret(source.path()),
        Err(RecoveryError::InstallationUnavailable)
    );
    fs::write(source.path().join("runtime.secret"), b"").expect("empty secret");
    assert_eq!(
        read_runtime_secret(source.path()),
        Err(RecoveryError::InstallationUnavailable)
    );
    fs::remove_file(source.path().join("runtime.secret")).expect("remove secret");
    assert_eq!(
        read_runtime_secret(source.path()),
        Err(RecoveryError::InstallationUnavailable)
    );
}

#[test]
fn recovery_error_maps_and_byte_totals_fail_closed() {
    assert_eq!(
        map_repository(RepositoryError::SchemaTooNew),
        RecoveryError::SchemaTooNew
    );
    for error in [
        RepositoryError::NotFound,
        RepositoryError::Conflict,
        RepositoryError::InvalidInput,
        RepositoryError::Unavailable,
    ] {
        assert_eq!(map_repository(error), RecoveryError::Database);
    }
    assert_eq!(
        map_storage(StorageError::IntegrityMismatch),
        RecoveryError::Integrity
    );
    for error in [
        StorageError::NotFound,
        StorageError::Conflict,
        StorageError::InvalidInput,
        StorageError::Unavailable,
    ] {
        assert_eq!(map_storage(error), RecoveryError::Storage);
    }
    assert_eq!(add_bytes(3, 4), Ok(7));
    assert_eq!(add_bytes(u64::MAX, 1), Err(RecoveryError::Integrity));
}

#[test]
fn backup_orchestration_propagates_every_post_snapshot_failure() {
    let cases = [
        (BackupFault::RemoveSnapshot, RecoveryError::Persistence),
        (BackupFault::BlockRuntimeSecret, RecoveryError::Persistence),
        (BackupFault::CorruptSnapshot, RecoveryError::Database),
        (BackupFault::DropInventoryTable, RecoveryError::Database),
        (BackupFault::RemoveStoredObject, RecoveryError::Storage),
        (BackupFault::OverflowByteTotal, RecoveryError::Integrity),
        (
            BackupFault::RemoveMetadataBeforeHash,
            RecoveryError::InvalidBackup,
        ),
        (
            BackupFault::RemoveSecretBeforeHash,
            RecoveryError::InvalidBackup,
        ),
        (BackupFault::BlockManifest, RecoveryError::Persistence),
        (BackupFault::BlockPersistence, RecoveryError::Persistence),
    ];
    for (index, (fault, expected)) in cases.into_iter().enumerate() {
        let source = test_support::installation();
        let parent = tempfile::tempdir().expect("parent");
        set_fault(fault);
        assert_eq!(
            crate::backup_data_directory(
                source.path(),
                &parent.path().join(format!("backup-{index}")),
                &crate::StorageConfiguration::Filesystem,
            ),
            Err(expected),
            "fault {fault:?}"
        );
    }

    let source = test_support::installation();
    let parent = tempfile::tempdir().expect("parent");
    let invalid_s3 = crate::test_support::invalid_s3_configuration();
    assert_eq!(
        crate::backup_data_directory(
            source.path(),
            &parent.path().join("invalid-s3"),
            &crate::StorageConfiguration::S3(invalid_s3),
        ),
        Err(RecoveryError::Storage)
    );
}
