#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::tests::fixture;
use super::*;
use blobyard_contract::{CredentialRepository, MigrationRepository};

#[test]
fn metadata_activation_maps_import_and_secret_failures() {
    let root = tempfile::tempdir().expect("root");
    let (_options, prepared, _downloaded, bootstrap) = fixture(root.path());

    let occupied =
        SqliteRepository::open(&root.path().join("occupied.sqlite3")).expect("occupied repository");
    occupied
        .import_migration(&prepared.snapshot)
        .expect("first import");
    assert_eq!(
        activate_metadata(&occupied, root.path(), &prepared, &bootstrap),
        Err(HostedMigrationError::Metadata)
    );

    let secret_root = tempfile::tempdir().expect("secret root");
    std::fs::create_dir(secret_root.path().join("runtime.secret")).expect("secret blocker");
    let secret_repository = SqliteRepository::open(&secret_root.path().join("metadata.sqlite3"))
        .expect("secret repository");
    assert_eq!(
        activate_metadata(
            &secret_repository,
            secret_root.path(),
            &prepared,
            &bootstrap
        ),
        Err(HostedMigrationError::Persistence)
    );
}

#[test]
fn metadata_activation_maps_existing_and_rejected_bootstrap_failures() {
    let root = tempfile::tempdir().expect("root");
    let (_options, prepared, _downloaded, bootstrap) = fixture(root.path());
    let bootstrap_root = tempfile::tempdir().expect("bootstrap root");
    let bootstrap_repository =
        SqliteRepository::open(&bootstrap_root.path().join("metadata.sqlite3"))
            .expect("bootstrap repository");
    assert!(
        bootstrap_repository
            .install_bootstrap(&"0".repeat(64))
            .expect("existing bootstrap")
    );
    assert_eq!(
        activate_metadata(
            &bootstrap_repository,
            bootstrap_root.path(),
            &prepared,
            &bootstrap
        ),
        Err(HostedMigrationError::Metadata)
    );

    let denied_root = tempfile::tempdir().expect("denied root");
    let denied_repository = SqliteRepository::open(&denied_root.path().join("metadata.sqlite3"))
        .expect("denied repository");
    rusqlite::Connection::open(denied_root.path().join("metadata.sqlite3"))
        .expect("secondary connection")
        .execute_batch(
            "CREATE TRIGGER deny_bootstrap_insert BEFORE INSERT ON bootstrap_authority \
             BEGIN SELECT RAISE(ABORT, 'denied'); END;",
        )
        .expect("bootstrap failure trigger");
    assert_eq!(
        activate_metadata(
            &denied_repository,
            denied_root.path(),
            &prepared,
            &bootstrap,
        ),
        Err(HostedMigrationError::Metadata)
    );
}

#[test]
fn activation_cleans_imported_objects_after_metadata_rejection() {
    let root = tempfile::tempdir().expect("root");
    assert_eq!(
        reject_activation(root.path(), None),
        Err(HostedMigrationError::Metadata)
    );
    assert!(!root.path().join("installation").exists());
}

#[test]
fn path_and_parent_sync_failures_are_redacted() {
    let root = tempfile::tempdir().expect("root");
    let loop_path = root.path().join("loop");
    std::os::unix::fs::symlink(&loop_path, &loop_path).expect("symlink loop");
    assert_eq!(
        reject_existing(&loop_path),
        Err(HostedMigrationError::Persistence)
    );
    assert_eq!(
        sync_parent(&root.path().join("missing")),
        Err(HostedMigrationError::Persistence)
    );
}

#[test]
fn activation_maps_every_filesystem_and_storage_transition_failure() {
    let root = tempfile::tempdir().expect("root");
    let (mut options, prepared, downloaded, bootstrap) = fixture(root.path());
    options.data_directory = Path::new("/").to_owned();
    assert_eq!(
        activate(&options, &prepared, &downloaded, &bootstrap),
        Err(HostedMigrationError::InvalidInput)
    );

    let (mut options, prepared, downloaded, bootstrap) = fixture(root.path());
    let blocked_parent = root.path().join("blocked-parent");
    std::fs::write(&blocked_parent, b"blocked").expect("parent blocker");
    options.data_directory = blocked_parent.join("installation");
    assert_eq!(
        activate(&options, &prepared, &downloaded, &bootstrap),
        Err(HostedMigrationError::Persistence)
    );

    for (fault, expected) in [
        (
            ActivationFault::BlockParentDirectory,
            HostedMigrationError::Persistence,
        ),
        (
            ActivationFault::BlockTemporaryDirectory,
            HostedMigrationError::Persistence,
        ),
        (
            ActivationFault::BlockInstallationDirectory,
            HostedMigrationError::Persistence,
        ),
        (
            ActivationFault::BlockMetadata,
            HostedMigrationError::Metadata,
        ),
        (ActivationFault::BlockStorage, HostedMigrationError::Storage),
        (
            ActivationFault::StorageNotEmpty,
            HostedMigrationError::StorageNotEmpty,
        ),
        (ActivationFault::Rename, HostedMigrationError::Persistence),
        (
            ActivationFault::ReopenStorage,
            HostedMigrationError::Persistence,
        ),
        (
            ActivationFault::RenameCleanup,
            HostedMigrationError::Persistence,
        ),
    ] {
        let isolated = tempfile::tempdir().expect("isolated root");
        let (mut options, prepared, downloaded, bootstrap) = fixture(isolated.path());
        if fault == ActivationFault::BlockParentDirectory {
            options.data_directory = isolated.path().join("blocked").join("installation");
        }
        let result = with_fault(fault, || {
            activate(&options, &prepared, &downloaded, &bootstrap)
        });
        assert_eq!(result, Err(expected), "{fault:?}");
    }
}

#[test]
fn activation_surfaces_cleanup_failure_after_metadata_rejection() {
    let root = tempfile::tempdir().expect("root");
    assert_eq!(
        reject_activation(root.path(), Some(ActivationFault::Cleanup)),
        Err(HostedMigrationError::Persistence)
    );
}

fn reject_activation(
    root: &std::path::Path,
    fault: Option<ActivationFault>,
) -> Result<(), HostedMigrationError> {
    let (options, mut prepared, downloaded, bootstrap) = fixture(root);
    prepared.snapshot.workspaces[0].name.clear();
    fault.map_or_else(
        || activate(&options, &prepared, &downloaded, &bootstrap),
        |active| {
            with_fault(active, || {
                activate(&options, &prepared, &downloaded, &bootstrap)
            })
        },
    )
}
