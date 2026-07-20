#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    RecoveryError, backup_data_directory, encode, restore_data_directory, rollback_preflight,
    test_support, upgrade_preflight,
};
use crate::StorageConfiguration;
use blobyard_contract::{MetadataRepositoryInventory, ObjectStorage, StorageKey};
use blobyard_repository_sqlite::SqliteRepository;
use blobyard_storage_filesystem::FilesystemStorage;
use rusqlite::Connection;
use std::io::Read;

const CONTENT: &[u8] = test_support::CONTENT;

fn installation() -> tempfile::TempDir {
    test_support::installation()
}

#[test]
fn backup_restore_and_preflight_preserve_exact_bytes_and_metadata() {
    let source = installation();
    let parent = tempfile::tempdir().expect("parent");
    let backup = parent.path().join("backup");
    let restored = parent.path().join("restored");

    let backup_report =
        backup_data_directory(source.path(), &backup, &StorageConfiguration::Filesystem)
            .expect("backup");
    assert!(backup_report.contains("\"destinationReady\": true"));
    let upgrade = upgrade_preflight(source.path()).expect("upgrade preflight");
    assert!(upgrade.contains("\"backupRequired\": true"));
    assert!(
        rollback_preflight(source.path())
            .expect("rollback")
            .contains("codeOnly")
    );

    let restore_report =
        restore_data_directory(&backup, &restored, &StorageConfiguration::Filesystem)
            .expect("restore");
    assert!(restore_report.contains("\"installationReady\": true"));
    let repository =
        SqliteRepository::open(&restored.join("metadata.sqlite3")).expect("restored repository");
    let records = repository.list_object_versions().expect("records");
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].checksum.as_deref(),
        Some(sha256(CONTENT).as_str())
    );
    let storage = FilesystemStorage::open(&restored.join("objects")).expect("restored storage");
    let mut read = storage
        .get(
            &StorageKey::new("objects/version_recovery").expect("key"),
            None,
        )
        .expect("restored object")
        .reader;
    let mut bytes = Vec::new();
    read.read_to_end(&mut bytes).expect("bytes");
    assert_eq!(bytes, CONTENT);
    assert_eq!(
        std::fs::read(restored.join("runtime.secret")).expect("restored secret"),
        std::fs::read(source.path().join("runtime.secret")).expect("source secret")
    );
}

#[test]
fn recovery_refuses_existing_destinations_active_uploads_and_bad_schemas() {
    let source = installation();
    let parent = tempfile::tempdir().expect("parent");
    let existing = parent.path().join("existing");
    std::fs::create_dir(&existing).expect("existing destination");
    assert_eq!(
        backup_data_directory(source.path(), &existing, &StorageConfiguration::Filesystem,),
        Err(RecoveryError::DestinationExists)
    );

    Connection::open(source.path().join("metadata.sqlite3"))
        .expect("database")
        .execute(
            "INSERT INTO object_versions
             (id, project_id, object_path, version, storage_key, state, created_at_ms)
             VALUES ('version_pending', 'project_recovery', 'pending.bin', 1,
                     'objects/version_pending', 'pending', 2)",
            [],
        )
        .expect("pending version");
    assert_eq!(
        backup_data_directory(
            source.path(),
            &parent.path().join("active"),
            &StorageConfiguration::Filesystem,
        ),
        Err(RecoveryError::ActiveUploads)
    );

    let schema = source.path().join("metadata.sqlite3");
    Connection::open(&schema)
        .expect("database")
        .pragma_update(None, "user_version", 0)
        .expect("old schema");
    assert_eq!(
        upgrade_preflight(source.path()),
        Err(RecoveryError::SchemaTooOld)
    );
    assert_eq!(
        rollback_preflight(source.path()),
        Err(RecoveryError::RollbackUnsafe)
    );
    Connection::open(&schema)
        .expect("database")
        .pragma_update(
            None,
            "user_version",
            blobyard_repository_sqlite::current_schema_version() + 1,
        )
        .expect("new schema");
    assert_eq!(
        upgrade_preflight(source.path()),
        Err(RecoveryError::SchemaTooNew)
    );
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    blobyard_core::hex_digest(&Sha256::digest(bytes))
}

#[test]
fn backup_rejects_missing_installation_and_secret() {
    let parent = tempfile::tempdir().expect("parent");
    assert_eq!(
        backup_data_directory(
            &parent.path().join("absent"),
            &parent.path().join("missing"),
            &StorageConfiguration::Filesystem,
        ),
        Err(RecoveryError::InstallationUnavailable)
    );

    let source = installation();
    std::fs::remove_file(source.path().join("runtime.secret")).expect("remove secret");
    assert_eq!(
        backup_data_directory(
            source.path(),
            &parent.path().join("no-secret"),
            &StorageConfiguration::Filesystem,
        ),
        Err(RecoveryError::InstallationUnavailable)
    );
}

#[test]
fn backup_rejects_unsupported_schema_and_unavailable_metadata() {
    let parent = tempfile::tempdir().expect("parent");
    let source = installation();
    Connection::open(source.path().join("metadata.sqlite3"))
        .expect("database")
        .pragma_update(
            None,
            "user_version",
            blobyard_repository_sqlite::oldest_supported_schema_version(),
        )
        .expect("old schema");
    assert_eq!(
        backup_data_directory(
            source.path(),
            &parent.path().join("old-schema"),
            &StorageConfiguration::Filesystem,
        ),
        Err(RecoveryError::SchemaTooOld)
    );

    let source = installation();
    Connection::open(source.path().join("metadata.sqlite3"))
        .expect("database")
        .pragma_update(
            None,
            "user_version",
            blobyard_repository_sqlite::current_schema_version() + 1,
        )
        .expect("new schema");
    assert_eq!(
        backup_data_directory(
            source.path(),
            &parent.path().join("new-schema"),
            &StorageConfiguration::Filesystem,
        ),
        Err(RecoveryError::SchemaTooNew)
    );

    let source = installation();
    std::fs::remove_file(source.path().join("metadata.sqlite3")).expect("remove database");
    assert_eq!(
        backup_data_directory(
            source.path(),
            &parent.path().join("no-database"),
            &StorageConfiguration::Filesystem,
        ),
        Err(RecoveryError::Database)
    );
}

#[test]
fn recovery_errors_have_stable_operator_safe_messages_and_encoding_failures() {
    let cases = [
        (
            RecoveryError::InstallationUnavailable,
            "standalone installation is unavailable or unsafe",
        ),
        (
            RecoveryError::DestinationExists,
            "recovery destination already exists",
        ),
        (
            RecoveryError::InvalidBackup,
            "backup is malformed, unsupported, or inconsistent",
        ),
        (
            RecoveryError::Database,
            "metadata snapshot is unavailable or corrupt",
        ),
        (
            RecoveryError::SchemaTooOld,
            "metadata schema is older than this binary can upgrade",
        ),
        (
            RecoveryError::SchemaTooNew,
            "metadata schema is newer than this binary supports",
        ),
        (
            RecoveryError::ActiveUploads,
            "backup requires every object upload to be terminal",
        ),
        (RecoveryError::Storage, "object storage is unavailable"),
        (
            RecoveryError::Integrity,
            "recovery bytes or integrity metadata do not match",
        ),
        (
            RecoveryError::StorageNotEmpty,
            "restore requires an empty object-storage namespace",
        ),
        (
            RecoveryError::RollbackUnsafe,
            "rollback binary does not exactly support the current schema",
        ),
        (
            RecoveryError::Persistence,
            "recovery staging data could not be persisted atomically",
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
    let unsupported_key = std::collections::BTreeMap::from([((1_u8, 2_u8), "value")]);
    assert_eq!(encode(&unsupported_key), Err(RecoveryError::Persistence));
}
