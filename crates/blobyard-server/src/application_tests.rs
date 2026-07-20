#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    default_workspace, enforce_retention, enforce_retention_with_storage, initialize,
    initialize_with_origin, initialize_with_origins, initialize_with_storage_origins_at,
    project_workspace, serve_until,
};
use crate::{
    ServerError, StorageConfiguration, create_runtime_secret, normalize_origin, runtime_secret,
};
use blobyard_contract::{
    AuditValue, LifecycleRepository, MetadataRepository, NewAuditEvent, ProjectRecord,
    RepositoryError, RetentionPolicyRecord,
};
use blobyard_core::Slug;
use blobyard_repository_sqlite::SqliteRepository;
use rusqlite::Connection;
use std::net::SocketAddr;
use tempfile::TempDir;

fn loopback() -> SocketAddr {
    "127.0.0.1:0".parse().expect("loopback address")
}

fn slug(value: &str) -> Slug {
    Slug::new(value.to_owned()).expect("fixture slug")
}

fn repository(root: &TempDir) -> SqliteRepository {
    SqliteRepository::open(&root.path().join("metadata.sqlite3")).expect("repository")
}

fn audit_event(workspace_id: &str) -> NewAuditEvent {
    NewAuditEvent {
        id: "audit_fixture".to_owned(),
        workspace_id: workspace_id.to_owned(),
        actor: "token_fixture".to_owned(),
        action: "retention.policy_set".to_owned(),
        request_id: "request_fixture".to_owned(),
        target_type: "retention_policy".to_owned(),
        metadata: vec![("fixture".to_owned(), AuditValue::Boolean(true))],
        created_at_ms: 1,
    }
}

#[test]
fn public_initialization_and_empty_retention_enforcement_round_trip() {
    let root = TempDir::new().expect("root");
    let mut initialized = initialize(root.path()).expect("initialization");
    let _router = initialized.router();
    assert!(initialized.take_bootstrap_token().is_some());
    assert!(initialized.take_bootstrap_token().is_none());

    let mut reopened = initialize_with_origin(root.path(), "https://example.com")
        .expect("reopened initialization");
    assert!(reopened.take_bootstrap_token().is_none());

    let repository = repository(&root);
    let workspace = default_workspace(&repository).expect("default workspace");
    let project = ProjectRecord {
        id: "project_retained".to_owned(),
        workspace_id: workspace.id.clone(),
        name: "Retained".to_owned(),
        slug: slug("retained"),
    };
    repository.create_project(&project).expect("project");
    repository
        .set_retention(
            &RetentionPolicyRecord {
                project_id: project.id.clone(),
                keep_latest: 1,
                path_glob: None,
                branch_glob: None,
                created_at_ms: 1,
                updated_at_ms: 1,
            },
            &audit_event(&workspace.id),
        )
        .expect("retention policy");

    enforce_retention(root.path()).expect("retention enforcement");
    enforce_retention_with_storage(root.path(), &StorageConfiguration::Filesystem)
        .expect("explicit retention enforcement");
    let run = repository
        .retention_overview(&project.id)
        .expect("retention overview")
        .last_run
        .expect("retention run");
    assert_eq!(run.status, "complete");
    assert_eq!(run.deleted_count, 0);
}

#[tokio::test]
async fn standalone_serve_binds_reopens_and_honors_shutdown() {
    let root = TempDir::new().expect("root");
    for _attempt in 0..2 {
        serve_until(loopback(), root.path(), None, None, Box::pin(async {}))
            .await
            .expect("graceful server shutdown");
    }
    let mut reopened = initialize(root.path()).expect("reopen initialized server");
    assert!(reopened.take_bootstrap_token().is_none());
}

#[tokio::test]
async fn standalone_serve_propagates_listener_bind_failures() {
    let root = TempDir::new().expect("root");
    let listener = tokio::net::TcpListener::bind(loopback())
        .await
        .expect("occupied listener");
    let occupied = listener.local_addr().expect("occupied address");
    assert!(
        serve_until(
            occupied,
            root.path(),
            Some("http://127.0.0.1:8787"),
            None,
            Box::pin(async {}),
        )
        .await
        .is_err()
    );
}

#[test]
fn initialization_rejects_blocked_paths_and_invalid_runtime_secrets() {
    let root = TempDir::new().expect("root");
    let blocked = root.path().join("blocked");
    std::fs::write(&blocked, b"file").expect("blocker");
    assert!(initialize(&blocked).is_err());

    let root = TempDir::new().expect("root");
    std::fs::create_dir(root.path().join("metadata.sqlite3")).expect("metadata blocker");
    assert!(initialize(root.path()).is_err());

    let root = TempDir::new().expect("root");
    let repository = repository(&root);
    drop(repository);
    std::fs::write(root.path().join("staging"), b"blocker").expect("staging blocker");
    assert!(initialize(root.path()).is_err());

    let root = TempDir::new().expect("root");
    std::fs::write(root.path().join("runtime.secret"), b"").expect("invalid secret");
    assert!(initialize(root.path()).is_err());
}

#[test]
fn origin_and_secret_helpers_cover_valid_existing_and_failure_paths() {
    assert_eq!(
        normalize_origin("https://example.com/").expect("origin"),
        "https://example.com"
    );
    for origin in [
        "not a url",
        "ftp://example.com/",
        "https://user@example.com/",
        "https://example.com/path",
        "https://example.com/?query=1",
        "https://example.com/#fragment",
    ] {
        assert!(initialize_with_origin(TempDir::new().expect("root").path(), origin).is_err());
    }
    assert_eq!(
        initialize_with_origins(
            TempDir::new().expect("root").path(),
            "http://127.0.0.1:8787",
            "https://yards.example.com/path",
        )
        .err(),
        Some(super::ServerError::WebYardOrigin)
    );

    let root = TempDir::new().expect("root");
    std::fs::write(root.path().join("runtime.secret"), b"existing-secret")
        .expect("existing secret");
    assert_eq!(
        runtime_secret(root.path())
            .expect("existing")
            .expose_secret(),
        "existing-secret"
    );
    std::fs::remove_file(root.path().join("runtime.secret")).expect("remove secret");
    std::fs::create_dir(root.path().join("runtime.secret")).expect("secret blocker");
    assert!(runtime_secret(root.path()).is_err());

    let root = TempDir::new().expect("root");
    let path = root.path().join("runtime.secret");
    std::fs::write(&path, b"winner").expect("winner");
    assert_eq!(
        create_runtime_secret(root.path(), &path)
            .expect("collision reads winner")
            .expose_secret(),
        "winner"
    );
    let missing_parent = root.path().join("missing/runtime.secret");
    assert!(create_runtime_secret(root.path(), &missing_parent).is_err());

    let file = root.path().join("not-a-directory");
    std::fs::write(&file, b"blocker").expect("blocker");
    assert!(create_runtime_secret(&file, &file.join("runtime.secret")).is_err());
}

#[test]
fn initialization_fails_closed_for_cleanup_clock_and_repository_failures() {
    let clock = TempDir::new().expect("clock root");
    assert!(matches!(
        initialize_with_storage_origins_at(
            clock.path(),
            "http://127.0.0.1:8787",
            "http://localhost:8787",
            &StorageConfiguration::Filesystem,
            Err(ServerError::Initialization),
        ),
        Err(ServerError::Initialization)
    ));
    assert!(!clock.path().join("runtime.secret").exists());

    let storage_failure = TempDir::new().expect("storage root");
    let invalid_s3 = crate::test_support::invalid_s3_configuration();
    assert!(matches!(
        initialize_with_storage_origins_at(
            storage_failure.path(),
            "http://127.0.0.1:8787",
            "http://localhost:8787",
            &StorageConfiguration::S3(invalid_s3),
            Ok(1),
        ),
        Err(ServerError::Storage)
    ));

    let repository_failure = TempDir::new().expect("repository root");
    initialize(repository_failure.path()).expect("initialization");
    repository(&repository_failure)
        .test_connection()
        .expect("connection")
        .execute("DROP TABLE deletion_operations", [])
        .expect("break cleanup lookup");
    assert!(matches!(
        initialize_with_storage_origins_at(
            repository_failure.path(),
            "http://127.0.0.1:8787",
            "http://localhost:8787",
            &StorageConfiguration::Filesystem,
            Ok(1),
        ),
        Err(ServerError::Repository(RepositoryError::Unavailable))
    ));
}

#[test]
fn workspace_helpers_find_create_and_fail_closed_on_provider_errors() {
    let root = TempDir::new().expect("root");
    let repository = repository(&root);
    let created = default_workspace(&repository).expect("default workspace");
    assert_eq!(created.slug, slug("default"));
    assert_eq!(default_workspace(&repository).expect("existing"), created);

    let renamed = blobyard_contract::WorkspaceRecord {
        name: "Personal".to_owned(),
        slug: slug("personal"),
        ..created.clone()
    };
    repository
        .rename_workspace(
            &renamed,
            &NewAuditEvent {
                id: "audit_workspace_renamed".to_owned(),
                workspace_id: created.id,
                actor: "token_fixture".to_owned(),
                action: "workspace.renamed".to_owned(),
                request_id: "request_workspace_renamed".to_owned(),
                target_type: "workspace".to_owned(),
                metadata: vec![(
                    "previousSlug".to_owned(),
                    AuditValue::String("default".to_owned()),
                )],
                created_at_ms: 1,
            },
        )
        .expect("rename default workspace");
    assert_eq!(
        default_workspace(&repository).expect("renamed workspace"),
        renamed
    );

    let project = ProjectRecord {
        id: "project_fixture".to_owned(),
        workspace_id: renamed.id.clone(),
        name: "Fixture".to_owned(),
        slug: slug("fixture"),
    };
    repository.create_project(&project).expect("project");
    assert_eq!(
        project_workspace(&repository, "project_fixture").expect("workspace"),
        renamed.id
    );
    assert!(project_workspace(&repository, "missing").is_err());

    let database = root.path().join("metadata.sqlite3");
    Connection::open(database)
        .expect("connection")
        .execute_batch("DROP TABLE audit_events; DROP TABLE projects; DROP TABLE workspaces;")
        .expect("drop table");
    assert!(default_workspace(&repository).is_err());
    assert!(project_workspace(&repository, "project_fixture").is_err());
}
