#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use crate::hosted_migration::{export::DownloadedObjects, projection::SourceObject};
use blobyard_contract::{
    MetadataRepository, MigrationObjectRecord, MigrationSnapshot, ObjectSource, ObjectStorage,
    ProjectRecord, StorageError, WorkspaceRecord,
};
use blobyard_core::{GeneratedSecretKind, Slug};
use blobyard_storage_filesystem::FilesystemStorage;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use super::test_storage::{FaultStorage, PutBehavior};

pub(super) fn fixture(
    root: &Path,
) -> (
    HostedMigrationOptions,
    PreparedMigration,
    DownloadedObjects,
    SecretString,
) {
    let download_root = tempfile::tempdir().expect("download root");
    let path = download_root.path().join("version_fixture");
    std::fs::write(&path, b"abc").expect("object bytes");
    let checksum = blobyard_core::hex_digest(&Sha256::digest(b"abc"));
    let object = MigrationObjectRecord {
        id: "version_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        object_path: "file.txt".to_owned(),
        version: 2,
        storage_key: "migration/version_fixture".to_owned(),
        size: 3,
        checksum: checksum.clone(),
        created_at_ms: 1,
        source: ObjectSource::Cli,
        git_repository: None,
        git_commit: None,
        git_branch: None,
        filename: "file.txt".to_owned(),
        content_type: "text/plain".to_owned(),
    };
    let prepared = PreparedMigration {
        snapshot: MigrationSnapshot {
            workspaces: vec![WorkspaceRecord {
                id: "workspace_default".to_owned(),
                name: "Default".to_owned(),
                slug: Slug::new("default").expect("slug"),
            }],
            projects: vec![ProjectRecord {
                id: "project_fixture".to_owned(),
                workspace_id: "workspace_default".to_owned(),
                name: "Project".to_owned(),
                slug: Slug::new("project").expect("slug"),
            }],
            objects: vec![object],
            shares: Vec::new(),
            retention: Vec::new(),
        },
        source_objects: vec![SourceObject {
            version_id: "version_fixture".to_owned(),
            uri: "blobyard://default/project/file.txt?version=2".to_owned(),
            size: 3,
            checksum,
        }],
        share_capabilities: Vec::new(),
    };
    let downloaded = DownloadedObjects {
        _temporary: download_root,
        paths: BTreeMap::from([("version_fixture".to_owned(), path)]),
    };
    let options = HostedMigrationOptions::new(
        "https://api.blobyard.com".to_owned(),
        root.join("installation"),
        "http://127.0.0.1:8787".to_owned(),
        Vec::new(),
        crate::StorageConfiguration::Filesystem,
    );
    let bootstrap = crate::auth::generate_token(GeneratedSecretKind::BootstrapToken);
    (options, prepared, downloaded, bootstrap)
}

#[test]
fn activation_persists_verified_bytes_metadata_and_runtime_authority() {
    let root = tempfile::tempdir().expect("root");
    let (options, prepared, downloaded, bootstrap) = fixture(root.path());

    activate(&options, &prepared, &downloaded, &bootstrap).expect("activate");

    let repository = SqliteRepository::open(&options.data_directory.join("metadata.sqlite3"))
        .expect("repository");
    assert_eq!(repository.list_workspaces().expect("workspaces").len(), 1);
    assert_eq!(
        repository
            .object_version("version_fixture")
            .expect("object")
            .version,
        2
    );
    assert!(options.data_directory.join("runtime.secret").is_file());
    let storage =
        FilesystemStorage::open(&options.data_directory.join("objects")).expect("storage");
    let read = storage
        .get(
            &StorageKey::new("migration/version_fixture").expect("key"),
            None,
        )
        .expect("stored object");
    let mut bytes = Vec::new();
    read.reader.take(4).read_to_end(&mut bytes).expect("bytes");
    assert_eq!(bytes, b"abc");
}

#[test]
fn activation_rejects_existing_destinations_and_integrity_disagreement() {
    let root = tempfile::tempdir().expect("root");
    let (options, prepared, downloaded, bootstrap) = fixture(root.path());
    std::fs::create_dir(&options.data_directory).expect("existing destination");
    assert_eq!(
        activate(&options, &prepared, &downloaded, &bootstrap),
        Err(HostedMigrationError::DestinationExists)
    );

    std::fs::remove_dir(&options.data_directory).expect("remove fixture destination");
    let mut invalid = prepared;
    invalid.snapshot.objects[0].checksum = "f".repeat(64);
    assert_eq!(
        activate(&options, &invalid, &downloaded, &bootstrap),
        Err(HostedMigrationError::Integrity)
    );
    assert!(!options.data_directory.exists());
}

#[test]
fn storage_emptiness_and_cleanup_are_explicit() {
    let temporary = tempfile::tempdir().expect("storage root");
    let storage = FilesystemStorage::open(temporary.path()).expect("storage");
    assert!(require_empty_storage(&storage).is_ok());
    let key = StorageKey::new("fixture/object").expect("key");
    storage
        .put(&key, &mut Cursor::new(b"abc"), None)
        .expect("object");
    assert_eq!(
        require_empty_storage(&storage),
        Err(HostedMigrationError::StorageNotEmpty)
    );
    cleanup(&storage, std::slice::from_ref(&key)).expect("cleanup");
    assert!(require_empty_storage(&storage).is_ok());
}

#[test]
fn storage_provider_failures_and_cleanup_failures_are_preserved() {
    let mut list_failure = FaultStorage::new(PutBehavior::Unavailable);
    list_failure.list_fails = true;
    assert_eq!(
        require_empty_storage(&list_failure),
        Err(HostedMigrationError::Storage)
    );

    let key = StorageKey::new("fixture/object").expect("key");
    let mut delete_failure = FaultStorage::new(PutBehavior::Unavailable);
    delete_failure.delete_fails = true;
    assert_eq!(
        cleanup(&delete_failure, &[key]),
        Err(HostedMigrationError::Persistence)
    );
}

#[test]
fn fault_storage_requires_the_expected_checksum_on_success() {
    let storage = FaultStorage::new(PutBehavior::SuccessThenMismatch);
    let key = StorageKey::new("fixture/object").expect("key");
    assert_eq!(
        storage.put(&key, &mut Cursor::new(b"abc"), None).err(),
        Some(StorageError::Unavailable)
    );
}

#[test]
fn object_import_rejects_missing_files_invalid_metadata_and_provider_results() {
    let root = tempfile::tempdir().expect("root");
    let (_options, prepared, downloaded, _bootstrap) = fixture(root.path());
    let storage = FilesystemStorage::open(&root.path().join("objects")).expect("storage");

    let missing = DownloadedObjects {
        _temporary: tempfile::tempdir().expect("missing root"),
        paths: BTreeMap::new(),
    };
    assert_eq!(
        import_objects(&storage, &prepared, &missing).err(),
        Some(HostedMigrationError::Integrity)
    );

    std::fs::remove_file(
        downloaded
            .paths
            .get("version_fixture")
            .expect("download path"),
    )
    .expect("remove download");
    assert_eq!(
        import_objects(&storage, &prepared, &downloaded).err(),
        Some(HostedMigrationError::Persistence)
    );

    let (_options, mut invalid_key, downloaded, _bootstrap) = fixture(root.path());
    invalid_key.snapshot.objects[0].storage_key = "../unsafe".to_owned();
    assert_eq!(
        import_objects(&storage, &invalid_key, &downloaded).err(),
        Some(HostedMigrationError::InvalidExport)
    );

    let (_options, mut invalid_checksum, downloaded, _bootstrap) = fixture(root.path());
    invalid_checksum.snapshot.objects[0].checksum = "invalid".to_owned();
    assert_eq!(
        import_objects(&storage, &invalid_checksum, &downloaded).err(),
        Some(HostedMigrationError::InvalidExport)
    );
}

#[test]
fn object_import_maps_metadata_integrity_and_provider_disagreement() {
    let root = tempfile::tempdir().expect("root");
    for (behavior, expected) in [
        (PutBehavior::Mismatch, HostedMigrationError::Integrity),
        (PutBehavior::Integrity, HostedMigrationError::Integrity),
        (PutBehavior::Unavailable, HostedMigrationError::Storage),
    ] {
        let (_options, prepared, downloaded, _bootstrap) = fixture(root.path());
        assert_eq!(
            import_objects(&FaultStorage::new(behavior), &prepared, &downloaded).err(),
            Some(expected)
        );
    }

    for behavior in [
        PutBehavior::SuccessThenMismatch,
        PutBehavior::SuccessThenIntegrity,
        PutBehavior::SuccessThenUnavailable,
    ] {
        let (_options, mut prepared, mut downloaded, _bootstrap) = fixture(root.path());
        let mut second = prepared.snapshot.objects[0].clone();
        second.id = "version_second".to_owned();
        second.storage_key = "migration/version_second".to_owned();
        prepared.snapshot.objects.push(second);
        let first_path = downloaded
            .paths
            .get("version_fixture")
            .expect("first path")
            .clone();
        downloaded
            .paths
            .insert("version_second".to_owned(), first_path);
        let mut storage = FaultStorage::new(behavior);
        storage.delete_fails = true;
        assert_eq!(
            import_objects(&storage, &prepared, &downloaded).err(),
            Some(HostedMigrationError::Persistence),
            "cleanup after {behavior:?}"
        );
    }
}
