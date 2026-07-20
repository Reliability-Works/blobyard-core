#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use blobyard_contract::{DeletionItem, DeletionPlan, MetadataRepository, ProjectRecord};
use blobyard_core::Slug;
use blobyard_repository_sqlite::SqliteRepository;
use blobyard_server::{ServerError, application_test_seams, enforce_retention, initialize};
use blobyard_storage_filesystem::FilesystemStorage;
use rusqlite::Connection;

fn repository(root: &tempfile::TempDir) -> SqliteRepository {
    SqliteRepository::open(&root.path().join("metadata.sqlite3")).expect("repository")
}

fn seed_namespace(repository: &SqliteRepository) -> ProjectRecord {
    application_test_seams::default_workspace(repository).expect("default workspace");
    let project = ProjectRecord {
        id: "project_fixture".to_owned(),
        workspace_id: "workspace_default".to_owned(),
        name: "Fixture".to_owned(),
        slug: Slug::new("fixture").expect("project slug"),
    };
    repository.create_project(&project).expect("project");
    project
}

fn plan(id: &str, items: Vec<DeletionItem>, complete: bool) -> DeletionPlan {
    DeletionPlan {
        id: id.to_owned(),
        items,
        complete,
        actor: "system:retention".to_owned(),
        request_id: "request_fixture".to_owned(),
    }
}

#[test]
fn completed_retention_plan_skips_storage_and_repository_finalization() {
    let root = tempfile::tempdir().expect("root");
    let repository = repository(&root);
    let storage = FilesystemStorage::open(&root.path().join("objects")).expect("storage");
    let completed = plan(
        "missing_completed_plan",
        vec![DeletionItem {
            version_id: "version_fixture".to_owned(),
            storage_key: "../invalid".to_owned(),
            version: 1,
        }],
        true,
    );

    assert_eq!(
        application_test_seams::enforce_plan(
            &repository,
            &storage,
            "workspace_fixture".to_owned(),
            1,
            completed,
        ),
        Ok(())
    );
}

#[test]
fn initialization_propagates_namespace_and_bootstrap_repository_failures() {
    let root = tempfile::tempdir().expect("root");
    drop(repository(&root));
    Connection::open(root.path().join("metadata.sqlite3"))
        .expect("connection")
        .execute_batch("DROP TABLE workspaces;")
        .expect("drop workspaces");
    assert_eq!(
        initialize(root.path()).err(),
        Some(ServerError::Repository(
            blobyard_contract::RepositoryError::Unavailable
        ))
    );

    let root = tempfile::tempdir().expect("root");
    drop(initialize(root.path()).expect("initialization"));
    Connection::open(root.path().join("metadata.sqlite3"))
        .expect("connection")
        .execute_batch("DROP TABLE bootstrap_authority;")
        .expect("drop bootstrap authority");
    assert_eq!(
        initialize(root.path()).err(),
        Some(ServerError::Repository(
            blobyard_contract::RepositoryError::Unavailable
        ))
    );
}

#[test]
fn retention_enforcement_propagates_open_and_clock_failures() {
    let blocked = tempfile::tempdir().expect("root");
    std::fs::create_dir(blocked.path().join("metadata.sqlite3")).expect("database blocker");
    assert!(matches!(
        enforce_retention(blocked.path()),
        Err(ServerError::Repository(_))
    ));

    let root = tempfile::tempdir().expect("root");
    let repository = repository(&root);
    let project = seed_namespace(&repository);
    let storage = FilesystemStorage::open(&root.path().join("objects")).expect("storage");
    assert_eq!(
        application_test_seams::enforce_project_clock_failure(&repository, &storage, &project.id,),
        Err(ServerError::Initialization)
    );
    assert_eq!(
        application_test_seams::enforce_plan_clock_failure(
            &repository,
            &storage,
            "workspace_default".to_owned(),
            1,
            plan("missing_plan", Vec::new(), false),
        ),
        Err(ServerError::Initialization)
    );
}

#[test]
fn retained_plan_rejects_corrupt_keys_and_preserves_fail_run_errors() {
    let root = tempfile::tempdir().expect("root");
    let repository = repository(&root);
    let storage = FilesystemStorage::open(&root.path().join("objects")).expect("storage");
    let corrupt = plan(
        "missing_plan",
        vec![DeletionItem {
            version_id: "version_fixture".to_owned(),
            storage_key: "../invalid".to_owned(),
            version: 1,
        }],
        false,
    );
    assert_eq!(
        application_test_seams::enforce_plan(
            &repository,
            &storage,
            "workspace_default".to_owned(),
            1,
            corrupt,
        ),
        Err(ServerError::Storage)
    );

    std::fs::create_dir_all(root.path().join("objects/objects/valid/key"))
        .expect("storage blocker");
    let storage_failure = plan(
        "missing_plan",
        vec![DeletionItem {
            version_id: "version_fixture".to_owned(),
            storage_key: "valid/key".to_owned(),
            version: 1,
        }],
        false,
    );
    assert_eq!(
        application_test_seams::enforce_plan(
            &repository,
            &storage,
            "workspace_default".to_owned(),
            1,
            storage_failure,
        ),
        Err(ServerError::Repository(
            blobyard_contract::RepositoryError::NotFound
        ))
    );
}

#[test]
fn namespace_helpers_propagate_invalid_and_provider_failures() {
    let root = tempfile::tempdir().expect("root");
    let repository = repository(&root);
    assert_eq!(
        application_test_seams::invalid_default_slug(&repository),
        Err(ServerError::Initialization)
    );
    Connection::open(root.path().join("metadata.sqlite3"))
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER fail_default BEFORE INSERT ON workspaces BEGIN SELECT RAISE(ABORT, 'fixture'); END;",
        )
        .expect("failure trigger");
    assert_eq!(
        application_test_seams::default_workspace(&repository),
        Err(ServerError::Repository(
            blobyard_contract::RepositoryError::Conflict
        ))
    );

    let root = tempfile::tempdir().expect("root");
    let second_repository =
        SqliteRepository::open(&root.path().join("metadata.sqlite3")).expect("second repository");
    seed_namespace(&second_repository);
    Connection::open(root.path().join("metadata.sqlite3"))
        .expect("connection")
        .execute_batch("DROP TABLE projects;")
        .expect("drop projects");
    assert_eq!(
        application_test_seams::project_workspace(&second_repository, "project_fixture"),
        Err(ServerError::Repository(
            blobyard_contract::RepositoryError::Unavailable
        ))
    );
}

#[test]
fn runtime_secret_write_failure_leaves_no_partial_authority() {
    let root = tempfile::tempdir().expect("root");
    let temporary = tempfile::NamedTempFile::new_in(root.path()).expect("temporary secret");
    assert_eq!(
        application_test_seams::runtime_secret_write_failure(root.path(), temporary),
        Err(ServerError::DataDirectory)
    );
    assert!(!root.path().join("runtime.secret").exists());
}
