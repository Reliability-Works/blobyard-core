#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::reconcile_data_directory;
use crate::StorageConfiguration;
use blobyard_contract::{
    MetadataRepository, NewObjectVersion, ObjectSource, ObjectStorage, ProjectRecord, StorageKey,
    WorkspaceRecord,
};
use blobyard_core::Slug;
use blobyard_repository_sqlite::SqliteRepository;
use blobyard_storage_filesystem::FilesystemStorage;
use sha2::{Digest, Sha256};
use std::io::Cursor;

fn fixture() -> (tempfile::TempDir, SqliteRepository, FilesystemStorage) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository = repository(temporary.path());
    let storage =
        FilesystemStorage::open(&temporary.path().join("objects")).expect("filesystem storage");
    (temporary, repository, storage)
}

#[test]
fn report_covers_every_finding_and_preserves_data() {
    let (temporary, repository, storage) = fixture();

    populate_findings(&repository, &storage);

    let before = std::fs::read(temporary.path().join("objects/objects/a-good")).expect("bytes");
    let encoded = reconcile_data_directory(temporary.path(), &StorageConfiguration::Filesystem)
        .expect("reconciliation");
    let report: serde_json::Value = serde_json::from_str(&encoded).expect("report JSON");

    assert_report_header(&report);
    assert_report_findings(&report);
    assert_eq!(
        std::fs::read(temporary.path().join("objects/objects/a-good")).expect("bytes"),
        before
    );
}

fn populate_findings(repository: &SqliteRepository, storage: &FilesystemStorage) {
    complete(repository, "good", "a-good", b"good");
    put(storage, "a-good", b"good");
    complete(repository, "missing", "b-missing", b"missing");
    complete(repository, "mismatch", "c-mismatch", b"expected");
    put(storage, "c-mismatch", b"actual");
    reserve(repository, "pending", "d-pending", 4);
    put(storage, "d-pending", b"pending bytes");
    reserve(repository, "pending-absent", "d-pending-absent", 5);
    put(storage, "e-no-metadata", b"orphan bytes");
    complete(repository, "no-integrity", "f-no-integrity", b"integrity");
    put(storage, "f-no-integrity", b"integrity");
    clear_integrity(repository, "no-integrity");
    insert_invalid(repository);
}

fn assert_report_header(report: &serde_json::Value) {
    assert_eq!(report["reportSchemaVersion"], 1);
    assert_eq!(report["coreVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(report["metadataSchemaVersion"], 16);
    assert_eq!(report["clean"], false);
    assert_eq!(report["counts"]["metadataRecords"], 7);
    assert_eq!(report["counts"]["physicalObjects"], 5);
    assert_eq!(report["counts"]["findings"], 6);
}

fn assert_report_findings(report: &serde_json::Value) {
    assert_eq!(report["missingBytes"][0]["storageKey"], "b-missing");
    assert_eq!(
        report["integrityDisagreements"][0]["storageKey"],
        "c-mismatch"
    );
    assert_eq!(
        report["integrityDisagreements"][0]["reason"],
        "content_mismatch"
    );
    assert_eq!(report["orphanedObjects"][0]["storageKey"], "d-pending");
    assert_eq!(report["missingMetadata"][0]["storageKey"], "e-no-metadata");
    assert_eq!(report["invalidMetadata"][0]["storageKey"], "../unsafe");
    assert_eq!(report["invalidMetadata"][0]["state"], "pending");
    assert_eq!(report["invalidMetadata"][1]["storageKey"], "f-no-integrity");
    assert_eq!(
        report["invalidMetadata"][1]["reason"],
        "missing_integrity_metadata"
    );
}

#[test]
fn clean_report_is_stable_and_storage_corruption_becomes_a_finding() {
    let (temporary, repository, storage) = fixture();
    complete(&repository, "good", "good", b"bytes");
    put(&storage, "good", b"bytes");

    let first = reconcile_data_directory(temporary.path(), &StorageConfiguration::Filesystem)
        .expect("first report");
    let second = reconcile_data_directory(temporary.path(), &StorageConfiguration::Filesystem)
        .expect("second report");
    assert_eq!(first, second);
    let report: serde_json::Value = serde_json::from_str(&first).expect("report");
    assert_eq!(report["clean"], true);
    assert_eq!(report["counts"]["findings"], 0);

    std::fs::write(
        temporary.path().join("objects/metadata/good.blobyard-meta"),
        b"invalid",
    )
    .expect("corrupt sidecar");
    let corrupt = reconcile_data_directory(temporary.path(), &StorageConfiguration::Filesystem)
        .expect("corrupt report");
    let report: serde_json::Value = serde_json::from_str(&corrupt).expect("report");
    assert_eq!(report["integrityDisagreements"][0]["storageKey"], "good");
    assert_eq!(
        report["integrityDisagreements"][0]["reason"],
        "storage_integrity_unreadable"
    );
    assert_eq!(
        report["integrityDisagreements"][0]["actualSize"],
        serde_json::Value::Null
    );
}

#[test]
fn inventory_and_repository_failures_do_not_emit_false_clean_reports() {
    let (temporary, repository, storage) = fixture();
    put(&storage, "valid", b"bytes");
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().expect("outside");
        std::os::unix::fs::symlink(
            outside.path(),
            temporary.path().join("objects/objects/unsafe-link"),
        )
        .expect("symlink");
        assert_eq!(
            reconcile_data_directory(temporary.path(), &StorageConfiguration::Filesystem,).err(),
            Some(crate::ServerError::Storage)
        );
        std::fs::remove_file(temporary.path().join("objects/objects/unsafe-link"))
            .expect("remove symlink");
    }
    repository
        .test_connection()
        .expect("connection")
        .execute_batch("DROP TABLE object_versions")
        .expect("drop table");
    assert_eq!(
        reconcile_data_directory(temporary.path(), &StorageConfiguration::Filesystem,).err(),
        Some(crate::ServerError::Repository(
            blobyard_contract::RepositoryError::Unavailable
        ))
    );
}

#[test]
fn storage_open_failure_does_not_emit_a_report() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    std::fs::write(temporary.path().join("objects"), b"block storage root")
        .expect("storage blocker");
    assert_eq!(
        reconcile_data_directory(temporary.path(), &StorageConfiguration::Filesystem).err(),
        Some(crate::ServerError::Storage)
    );
}

fn repository(data_directory: &std::path::Path) -> SqliteRepository {
    let repository =
        SqliteRepository::open(&data_directory.join("metadata.sqlite3")).expect("repository");
    repository
        .create_workspace(&WorkspaceRecord {
            id: "workspace".to_owned(),
            name: "Workspace".to_owned(),
            slug: Slug::new("workspace".to_owned()).expect("slug"),
        })
        .expect("workspace");
    repository
        .create_project(&ProjectRecord {
            id: "project".to_owned(),
            workspace_id: "workspace".to_owned(),
            name: "Project".to_owned(),
            slug: Slug::new("project".to_owned()).expect("slug"),
        })
        .expect("project");
    repository
}

fn reserve(repository: &SqliteRepository, id: &str, key: &str, version: u64) {
    repository
        .reserve_object_version(&NewObjectVersion {
            id: id.to_owned(),
            project_id: "project".to_owned(),
            object_path: format!("{id}.bin"),
            version,
            storage_key: key.to_owned(),
            source: ObjectSource::Cli,
            git_repository: None,
            git_commit: None,
            git_branch: None,
        })
        .expect("reserve version");
}

fn complete(repository: &SqliteRepository, id: &str, key: &str, bytes: &[u8]) {
    reserve(repository, id, key, id.bytes().map(u64::from).sum());
    repository
        .complete_object_version(id, bytes.len() as u64, &checksum(bytes))
        .expect("complete version");
}

fn put(storage: &FilesystemStorage, key: &str, bytes: &[u8]) {
    storage
        .put(
            &StorageKey::new(key).expect("key"),
            &mut Cursor::new(bytes),
            None,
        )
        .expect("put object");
}

fn checksum(bytes: &[u8]) -> String {
    blobyard_core::hex_digest(&Sha256::digest(bytes))
}

fn insert_invalid(repository: &SqliteRepository) {
    repository
        .test_connection()
        .expect("connection")
        .execute(
            "INSERT INTO object_versions (id, project_id, object_path, version, storage_key, state, created_at_ms, source) VALUES ('invalid', 'project', 'invalid.bin', 99, '../unsafe', 'pending', 1, 'cli')",
            [],
        )
        .expect("invalid metadata fixture");
}

fn clear_integrity(repository: &SqliteRepository, version_id: &str) {
    let connection = repository.test_connection().expect("connection");
    connection
        .execute_batch("PRAGMA ignore_check_constraints = ON")
        .expect("permit corruption fixture");
    connection
        .execute(
            "UPDATE object_versions SET size = NULL, checksum = NULL WHERE id = ?1",
            [version_id],
        )
        .expect("clear integrity metadata");
}
