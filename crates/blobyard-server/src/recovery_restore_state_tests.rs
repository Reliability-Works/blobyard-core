use super::*;

#[test]
fn empty_storage_and_seed_checks_fail_closed() {
    assert_eq!(
        require_empty_storage(&test_support::ScriptedStorage::empty()),
        Ok(())
    );
    assert_eq!(
        require_empty_storage(&test_support::ScriptedStorage::with_object(
            test_support::CONTENT
        )),
        Err(RecoveryError::StorageNotEmpty)
    );
    assert_eq!(
        require_empty_storage(
            &test_support::ScriptedStorage::empty().with_inventory_error(StorageError::Unavailable)
        ),
        Err(RecoveryError::Storage)
    );
    assert_eq!(
        seed_storage(&test_support::ScriptedStorage::empty(), "../invalid"),
        Err(RecoveryError::Storage)
    );
    assert_eq!(
        seed_storage(
            &test_support::ScriptedStorage::empty()
                .with_put_mode(test_support::PutMode::Error(StorageError::Unavailable)),
            "objects/recovery-fault",
        ),
        Err(RecoveryError::Storage)
    );
}

#[test]
fn imported_object_cleanup_preserves_the_stronger_failure() {
    let key = test_support::key(test_support::KEY);
    let storage = test_support::ScriptedStorage::with_object(test_support::CONTENT);
    assert_eq!(cleanup(&storage, std::slice::from_ref(&key)), Ok(()));
    assert_eq!(storage.object_count(), 0);
    let storage = test_support::ScriptedStorage::with_object(test_support::CONTENT)
        .with_delete_error(StorageError::Unavailable);
    assert_eq!(
        cleanup(&storage, std::slice::from_ref(&key)),
        Err(RecoveryError::Storage)
    );
    assert_eq!(
        cleanup_after(
            &storage,
            std::slice::from_ref(&key),
            RecoveryError::Integrity
        ),
        Err(RecoveryError::Storage)
    );
    let storage = test_support::ScriptedStorage::with_object(test_support::CONTENT);
    assert_eq!(
        cleanup_after(
            &storage,
            std::slice::from_ref(&key),
            RecoveryError::Integrity
        ),
        Err(RecoveryError::Integrity)
    );
}

#[test]
fn persistence_failures_remove_imported_objects() {
    let key = test_support::key(test_support::KEY);
    let parent = tempfile::tempdir().expect("parent");
    let destination = parent.path().join("destination");
    let stage = crate::recovery::io::create_stage(&destination).expect("stage");
    fs::create_dir(&destination).expect("race destination");
    fs::write(destination.join("occupied"), b"occupied").expect("occupy destination");
    let storage = test_support::ScriptedStorage::with_object(test_support::CONTENT);
    assert_eq!(
        persist_restored_stage(
            Ok(()),
            stage,
            &destination,
            &storage,
            std::slice::from_ref(&key),
        ),
        Err(RecoveryError::Persistence)
    );
    assert_eq!(storage.object_count(), 0);

    let destination = parent.path().join("failed-before-persist");
    let stage = crate::recovery::io::create_stage(&destination).expect("stage");
    let storage = test_support::ScriptedStorage::with_object(test_support::CONTENT);
    assert_eq!(
        persist_restored_stage(
            Err(RecoveryError::Integrity),
            stage,
            &destination,
            &storage,
            std::slice::from_ref(&key),
        ),
        Err(RecoveryError::Integrity)
    );
    assert_eq!(storage.object_count(), 0);
}

#[test]
fn byte_totals_are_checked_before_restore_activation() {
    assert_eq!(
        total_bytes(&valid_manifest()),
        Ok(test_support::CONTENT.len() as u64)
    );
    let overflow = BackupManifest::new(
        15,
        test_support::sha256(b"metadata"),
        test_support::sha256(b"runtime"),
        vec![
            BackupObject::new("objects/a".to_owned(), u64::MAX, test_support::sha256(b"a")),
            BackupObject::new("objects/b".to_owned(), 1, test_support::sha256(b"b")),
        ],
    );
    assert_eq!(total_bytes(&overflow), Err(RecoveryError::Integrity));
}

#[test]
fn restore_orchestration_rejects_invalid_backup_and_destination() {
    let parent = tempfile::tempdir().expect("parent");
    assert_eq!(
        apply(
            &parent.path().join("missing"),
            &parent.path().join("invalid-destination"),
            &crate::StorageConfiguration::Filesystem,
        )
        .expect_err("invalid backup must fail"),
        RecoveryError::InvalidBackup
    );

    let (_backup_parent, backup) = create_backup();
    let existing = parent.path().join("existing");
    fs::create_dir(&existing).expect("existing destination");
    assert_eq!(
        apply(&backup, &existing, &crate::StorageConfiguration::Filesystem)
            .expect_err("existing destination must fail"),
        RecoveryError::DestinationExists
    );
}

#[test]
fn restore_orchestration_propagates_each_transition_failure_and_cleans_up() {
    let parent = tempfile::tempdir().expect("parent");
    for (index, (fault, expected)) in [
        (
            RestoreFault::RemoveControlFile,
            RecoveryError::InvalidBackup,
        ),
        (RestoreFault::SeedStorage, RecoveryError::StorageNotEmpty),
        (RestoreFault::SeedStorageError, RecoveryError::Storage),
        (RestoreFault::CorruptObject, RecoveryError::Integrity),
        (RestoreFault::BlockPersistence, RecoveryError::Persistence),
        (
            RestoreFault::RemoveMetadataAfterHashes,
            RecoveryError::InvalidBackup,
        ),
        (
            RestoreFault::RemoveSecretAfterHashes,
            RecoveryError::InvalidBackup,
        ),
        (
            RestoreFault::RemoveObjectAfterHash,
            RecoveryError::InvalidBackup,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let (_backup_parent, backup) = create_backup();
        set_fault(fault);
        assert_eq!(
            apply(
                &backup,
                &parent.path().join(format!("restore-{index}")),
                &crate::StorageConfiguration::Filesystem,
            )
            .expect_err("restore fault must fail"),
            expected,
            "fault {fault:?}"
        );
    }
}

#[test]
fn restore_orchestration_rejects_overflow_and_invalid_storage_configuration() {
    let parent = tempfile::tempdir().expect("parent");
    let (_backup_parent, backup) = create_backup();
    let mut value = read_json(&backup);
    value["objects"] = serde_json::json!([
        {
            "storageKey": "objects/a",
            "size": u64::MAX,
            "checksum": test_support::sha256(b"a")
        },
        {
            "storageKey": "objects/b",
            "size": 1,
            "checksum": test_support::sha256(b"b")
        }
    ]);
    write_json(&backup, &value);
    assert_eq!(
        apply(
            &backup,
            &parent.path().join("overflow"),
            &crate::StorageConfiguration::Filesystem,
        )
        .expect_err("overflow must fail"),
        RecoveryError::Integrity
    );

    let invalid_s3 = crate::test_support::invalid_s3_configuration();
    let (_backup_parent, backup) = create_backup();
    assert_eq!(
        apply(
            &backup,
            &parent.path().join("invalid-s3"),
            &crate::StorageConfiguration::S3(invalid_s3),
        )
        .expect_err("invalid S3 configuration must fail"),
        RecoveryError::Storage
    );
}
