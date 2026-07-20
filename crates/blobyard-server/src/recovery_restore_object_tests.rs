use super::*;

fn object_fixture() -> (tempfile::TempDir, std::path::PathBuf, BackupManifest) {
    let root = tempfile::tempdir().expect("root");
    let backup = root.path().join("backup");
    fs::create_dir_all(backup.join("objects/objects")).expect("objects");
    fs::write(
        backup.join("objects").join(test_support::KEY),
        test_support::CONTENT,
    )
    .expect("object");
    (root, backup, valid_manifest())
}

#[test]
fn object_restore_accepts_valid_objects_and_rejects_invalid_identifiers() {
    let (_root, backup, manifest) = object_fixture();
    let storage = test_support::ScriptedStorage::empty();
    let mut imported = Vec::new();
    restore_objects(&backup, &storage, &manifest, &mut imported).expect("restore object");
    assert_eq!(imported, vec![test_support::key(test_support::KEY)]);

    let mut invalid_key = valid_manifest();
    invalid_key.objects[0].storage_key = "../escape".to_owned();
    assert_eq!(
        restore_objects(
            &backup,
            &test_support::ScriptedStorage::empty(),
            &invalid_key,
            &mut Vec::new()
        ),
        Err(RecoveryError::InvalidBackup)
    );

    let mut invalid_checksum = valid_manifest();
    invalid_checksum.objects[0].checksum = "not-a-checksum".to_owned();
    assert_eq!(
        restore_objects(
            &backup,
            &test_support::ScriptedStorage::empty(),
            &invalid_checksum,
            &mut Vec::new(),
        ),
        Err(RecoveryError::InvalidBackup)
    );
}

#[test]
fn object_restore_rejects_missing_tampered_and_unreadable_objects() {
    let (_root, backup, manifest) = object_fixture();
    let missing_root = tempfile::tempdir().expect("missing root");
    assert_eq!(
        restore_objects(
            missing_root.path(),
            &test_support::ScriptedStorage::empty(),
            &manifest,
            &mut Vec::new(),
        ),
        Err(RecoveryError::InvalidBackup)
    );

    let mut wrong_size = valid_manifest();
    wrong_size.objects[0].size += 1;
    assert_eq!(
        restore_objects(
            &backup,
            &test_support::ScriptedStorage::empty(),
            &wrong_size,
            &mut Vec::new()
        ),
        Err(RecoveryError::Integrity)
    );
    let mut wrong_checksum = valid_manifest();
    wrong_checksum.objects[0].checksum = test_support::sha256(b"different");
    assert_eq!(
        restore_objects(
            &backup,
            &test_support::ScriptedStorage::empty(),
            &wrong_checksum,
            &mut Vec::new(),
        ),
        Err(RecoveryError::Integrity)
    );
    assert_eq!(
        with_fault(IoFault::HashRead, || {
            restore_objects(
                &backup,
                &test_support::ScriptedStorage::empty(),
                &manifest,
                &mut Vec::new(),
            )
        }),
        Err(RecoveryError::Storage)
    );
}

#[test]
fn object_restore_classifies_provider_failures_and_inventory_drift() {
    let (_root, backup, manifest) = object_fixture();
    for error in [
        StorageError::NotFound,
        StorageError::Conflict,
        StorageError::InvalidInput,
        StorageError::IntegrityMismatch,
        StorageError::Unavailable,
    ] {
        let storage = test_support::ScriptedStorage::empty()
            .with_put_mode(test_support::PutMode::Error(error));
        assert_eq!(
            restore_objects(&backup, &storage, &manifest, &mut Vec::new()),
            Err(RecoveryError::Storage)
        );
    }

    let storage = test_support::ScriptedStorage::empty()
        .with_put_mode(test_support::PutMode::MetadataMismatch);
    assert_eq!(
        restore_objects(&backup, &storage, &manifest, &mut Vec::new()),
        Err(RecoveryError::Integrity)
    );
    let storage =
        test_support::ScriptedStorage::empty().with_inventory_error(StorageError::Unavailable);
    assert_eq!(
        restore_objects(&backup, &storage, &manifest, &mut Vec::new()),
        Err(RecoveryError::Storage)
    );
    let storage = test_support::ScriptedStorage::empty().with_inventory_extra("objects/extra");
    assert_eq!(
        restore_objects(&backup, &storage, &manifest, &mut Vec::new()),
        Err(RecoveryError::Integrity)
    );
}
