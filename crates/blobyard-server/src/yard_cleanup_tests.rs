#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::resume;
use crate::ServerError;
use blobyard_contract::{
    AuditValue, LifecycleRepository, NewAuditEvent, NewWebYard, NewYardDeploy, NewYardFile,
    ObjectStorage, StorageError, StorageKey, WebYardRepository,
};
use blobyard_core::Slug;
use blobyard_repository_sqlite::SqliteRepository;
use blobyard_storage_filesystem::FilesystemStorage;

const KEY: &str = "objects/yard-cleanup/1";
const BYTES: &[u8] = b"yard cleanup fixture";

struct Fixture {
    root: tempfile::TempDir,
    repository: SqliteRepository,
    storage: FilesystemStorage,
}

impl Fixture {
    fn new(write_bytes: bool) -> Self {
        let root = tempfile::tempdir().expect("root");
        let repository =
            SqliteRepository::open(&root.path().join("metadata.sqlite3")).expect("repository");
        blobyard_testkit::repository_conformance(&repository).expect("metadata fixture");
        blobyard_testkit::transfer_conformance(&repository, "project_fixture")
            .expect("transfer fixture");
        create_pending_cleanup(&repository);
        let storage = FilesystemStorage::open(&root.path().join("objects")).expect("storage");
        if write_bytes {
            storage
                .put(
                    &StorageKey::new(KEY).expect("key"),
                    &mut std::io::Cursor::new(BYTES),
                    None,
                )
                .expect("bytes");
        }
        Self {
            root,
            repository,
            storage,
        }
    }

    fn pending_count(&self) -> usize {
        self.repository
            .pending_yard_cleanups(None)
            .expect("pending cleanups")
            .len()
    }

    fn audit_actions(&self) -> Vec<String> {
        self.repository
            .list_audit("workspace_fixture", None, 100)
            .expect("audit")
            .items
            .into_iter()
            .map(|event| event.action)
            .collect()
    }
}

fn event(
    action: &str,
    target_type: &str,
    metadata: Vec<(String, AuditValue)>,
    created_at_ms: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{action}"),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{action}"),
        target_type: target_type.to_owned(),
        metadata,
        created_at_ms,
    }
}

fn yard_and_deploy() -> (NewWebYard, NewYardDeploy) {
    let yard = NewWebYard {
        id: "yard_cleanup_1".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: Slug::new("cleanup").expect("slug"),
        host_label: "cleanup-123456789-fixture-1".to_owned(),
        created_at_ms: 1,
    };
    let deploy = NewYardDeploy {
        id: "deploy_cleanup_1".to_owned(),
        yard_id: yard.id.clone(),
        workspace_id: yard.workspace_id.clone(),
        project_id: yard.project_id.clone(),
        client_deploy_id: "clientdeploy00000001".to_owned(),
        manifest_root: ".blobyard-yard/yard_cleanup_1/clientdeploy00000001/".to_owned(),
        deployment_host_label: "cleanup-0123456789-fixture-1".to_owned(),
        spa: false,
        clean_urls: false,
        created_at_ms: 1,
    };
    (yard, deploy)
}

fn insert_version(repository: &SqliteRepository, deploy: &NewYardDeploy) {
    repository
        .test_connection()
        .expect("connection")
        .execute(
            "INSERT INTO object_versions (id, project_id, object_path, version, storage_key, state, size, checksum, created_at_ms, source) VALUES ('version_yard_cleanup_1', 'project_fixture', ?1, 1, ?2, 'complete', ?3, ?4, 1, 'web')",
            rusqlite::params![
                format!("{}index.html", deploy.manifest_root),
                KEY,
                i64::try_from(BYTES.len()).expect("size"),
                "a".repeat(64),
            ],
        )
        .expect("version");
}

fn create_pending_cleanup(repository: &SqliteRepository) {
    let (yard, deploy) = yard_and_deploy();
    repository
        .start_yard_deploy(
            &yard,
            &deploy,
            &event(
                "yard.created",
                "web_yard",
                vec![("yardId".to_owned(), AuditValue::String(yard.id.clone()))],
                1,
            ),
        )
        .expect("start deploy");
    insert_version(repository, &deploy);
    repository
        .finalise_yard_deploy(
            &deploy.id,
            &[NewYardFile {
                normalized_path: "index.html".to_owned(),
                version_id: "version_yard_cleanup_1".to_owned(),
                byte_size: BYTES.len() as u64,
            }],
            2,
            &event(
                "yard.deployed",
                "yard_deploy",
                vec![
                    ("deployId".to_owned(), AuditValue::String(deploy.id.clone())),
                    ("fileCount".to_owned(), AuditValue::Number(1)),
                    ("status".to_owned(), AuditValue::String("live".to_owned())),
                    (
                        "totalBytes".to_owned(),
                        AuditValue::Number(BYTES.len() as u64),
                    ),
                ],
                2,
            ),
        )
        .expect("finalise deploy");
    repository
        .delete_web_yard(
            &yard.id,
            3,
            &event(
                "yard.deleted",
                "web_yard",
                vec![("yardId".to_owned(), AuditValue::String(yard.id.clone()))],
                3,
            ),
        )
        .expect("delete yard");
}

#[test]
fn startup_resume_deletes_bytes_and_finalises_metadata() {
    let fixture = Fixture::new(true);
    assert_eq!(fixture.pending_count(), 1);

    resume(&fixture.repository, &fixture.storage, 4).expect("resume");

    assert_eq!(fixture.pending_count(), 0);
    assert_eq!(
        fixture.storage.head(&StorageKey::new(KEY).expect("key")),
        Err(StorageError::NotFound)
    );
    assert!(
        fixture
            .audit_actions()
            .contains(&"yard.cleanup_completed".to_owned())
    );
}

#[test]
fn startup_resume_treats_already_missing_bytes_as_success() {
    let fixture = Fixture::new(false);
    resume(&fixture.repository, &fixture.storage, 4).expect("idempotent resume");
    assert_eq!(fixture.pending_count(), 0);
}

#[test]
fn startup_resume_fails_closed_for_corrupt_keys_and_storage_errors() {
    let invalid = Fixture::new(false);
    invalid
        .repository
        .test_connection()
        .expect("connection")
        .execute(
            "UPDATE deletion_items SET storage_key = '../invalid' WHERE operation_id = 'yardcleanup_deploy_cleanup_1'",
            [],
        )
        .expect("corrupt key");
    assert_eq!(
        resume(&invalid.repository, &invalid.storage, 4),
        Err(ServerError::Initialization)
    );
    assert_eq!(invalid.pending_count(), 1);

    let unavailable = Fixture::new(true);
    let object_path = unavailable
        .root
        .path()
        .join("objects/objects/objects/yard-cleanup/1");
    std::fs::remove_file(&object_path).expect("remove fixture object");
    std::fs::create_dir(&object_path).expect("block deletion");
    assert_eq!(
        resume(&unavailable.repository, &unavailable.storage, 4),
        Err(ServerError::Storage)
    );
    assert_eq!(unavailable.pending_count(), 1);
}

#[test]
fn startup_resume_surfaces_repository_failures_before_and_after_byte_deletion() {
    let lookup = Fixture::new(false);
    lookup
        .repository
        .test_connection()
        .expect("connection")
        .execute("DROP TABLE deletion_operations", [])
        .expect("break lookup");
    assert!(matches!(
        resume(&lookup.repository, &lookup.storage, 4),
        Err(ServerError::Repository(_))
    ));

    let finalise = Fixture::new(true);
    finalise
        .repository
        .test_connection()
        .expect("connection")
        .execute("DROP TABLE audit_events", [])
        .expect("break finalise");
    assert!(matches!(
        resume(&finalise.repository, &finalise.storage, 4),
        Err(ServerError::Repository(_))
    ));
    assert_eq!(finalise.pending_count(), 1);
    assert_eq!(
        finalise.storage.head(&StorageKey::new(KEY).expect("key")),
        Err(StorageError::NotFound)
    );
}

#[path = "yard_cleanup_request_tests.rs"]
mod request_tests;
