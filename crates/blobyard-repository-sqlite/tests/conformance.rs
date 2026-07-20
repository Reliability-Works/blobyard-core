#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! `SQLite` repository conformance and migration guards.

use blobyard_contract::{
    AuditValue, CiAction, CiRepository, CredentialRepository, LocalCiTrustRecord,
    MetadataRepository, NewAuditEvent, NewDownloadGrant, ProjectRecord, RepositoryError,
    TransferRepository, WorkspaceRecord,
};
use blobyard_core::Slug;
use blobyard_repository_sqlite::SqliteRepository;

const INITIAL_SCHEMA: &str = include_str!("../migrations/0001_initial.sql");
const LOCAL_AUTH_SCHEMA: &str = include_str!("../migrations/0002_local_auth.sql");
const TRANSFER_SCHEMA: &str = include_str!("../migrations/0003_transfers.sql");
const DOWNLOAD_SCHEMA: &str = include_str!("../migrations/0004_downloads.sql");
const LIFECYCLE_SCHEMA: &str = include_str!("../migrations/0005_object_lifecycle.sql");

fn workspace(id: &str, name: &str, slug: &str) -> WorkspaceRecord {
    WorkspaceRecord {
        id: id.to_owned(),
        name: name.to_owned(),
        slug: Slug::new(slug).expect("workspace slug"),
    }
}

fn cross_workspace_event(owner: &WorkspaceRecord, trust: &LocalCiTrustRecord) -> NewAuditEvent {
    NewAuditEvent {
        id: "audit_cross_workspace".to_owned(),
        workspace_id: owner.id.clone(),
        actor: "token_operator".to_owned(),
        action: "ci.trust_created".to_owned(),
        request_id: "request_cross_workspace".to_owned(),
        target_type: "ci_trust".to_owned(),
        metadata: vec![
            (
                "repository".to_owned(),
                AuditValue::String(trust.repository.clone()),
            ),
            ("targetId".to_owned(), AuditValue::String(trust.id.clone())),
        ],
        created_at_ms: trust.created_at_ms,
    }
}

fn yard_fixture() -> blobyard_testkit::YardConformanceFixture {
    blobyard_testkit::YardConformanceFixture::new("docs", "inactive", "history")
        .expect("Yard conformance fixture")
}

#[test]
fn sqlite_satisfies_the_metadata_contract() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("blobyard.sqlite3");
    let repository = SqliteRepository::open(&path).expect("repository");
    blobyard_testkit::repository_conformance(&repository).expect("conformance");
    let workspace = repository
        .list_workspaces()
        .expect("workspaces")
        .into_iter()
        .next()
        .expect("fixture workspace");
    blobyard_testkit::credential_conformance(&repository, &workspace.id)
        .expect("credential conformance");
    blobyard_testkit::transfer_conformance(&repository, "project_fixture")
        .expect("transfer conformance");
    blobyard_testkit::yard_conformance(&repository, &yard_fixture()).expect("Web Yard conformance");
    blobyard_testkit::inbox_conformance(&repository).expect("inbox conformance");
    blobyard_testkit::lifecycle_conformance(&repository).expect("lifecycle conformance");
    drop(repository);
    let reopened = SqliteRepository::open(&path).expect("reopened repository");
    assert_eq!(reopened.schema_version().expect("schema version"), 16);
}

#[test]
fn ci_trust_creation_rejects_a_project_owned_by_another_workspace() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("ci-scope.sqlite3");
    let repository = SqliteRepository::open(&path).expect("repository");
    let owner = workspace("workspace_owner", "Owner", "owner");
    let foreign = workspace("workspace_foreign", "Foreign", "foreign");
    repository
        .create_workspace(&owner)
        .expect("owner workspace");
    repository
        .create_workspace(&foreign)
        .expect("foreign workspace");
    let foreign_project = ProjectRecord {
        id: "project_foreign".to_owned(),
        workspace_id: foreign.id,
        name: "Foreign project".to_owned(),
        slug: Slug::new("foreign-project").expect("project slug"),
    };
    repository
        .create_project(&foreign_project)
        .expect("foreign project");
    let trust = LocalCiTrustRecord {
        id: "trust_cross_workspace".to_owned(),
        workspace_id: owner.id.clone(),
        project_id: Some(foreign_project.id),
        repository: "reliability-works/blobyard-core".to_owned(),
        workflow_path: ".github/workflows/release.yml".to_owned(),
        workflow_ref: "refs/heads/main".to_owned(),
        allowed_ref_glob: "refs/heads/main".to_owned(),
        environment: None,
        allowed_actions: vec![CiAction::Upload],
        audience: "https://api.blobyard.local".to_owned(),
        created_at_ms: 10,
        revoked_at_ms: None,
    };
    let event = cross_workspace_event(&owner, &trust);
    let mut invalid_trust = trust.clone();
    invalid_trust.repository.clear();
    assert_eq!(
        repository.create_ci_trust(&invalid_trust, &event),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        repository.create_ci_trust(&trust, &event),
        Err(RepositoryError::NotFound)
    );
    assert!(
        repository
            .list_ci_trusts(&owner.id)
            .expect("owner trusts")
            .is_empty()
    );
}

#[test]
fn sqlite_refuses_a_newer_schema() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("future.sqlite3");
    let connection = rusqlite::Connection::open(&path).expect("sqlite");
    connection
        .pragma_update(None, "user_version", 17)
        .expect("future version");
    drop(connection);
    assert_eq!(
        SqliteRepository::open(&path).expect_err("newer schema rejected"),
        blobyard_contract::RepositoryError::SchemaTooNew
    );
}

#[test]
fn sqlite_migrates_v5_tokens_with_safe_lifecycle_defaults() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("v5.sqlite3");
    let connection = rusqlite::Connection::open(&path).expect("sqlite");
    for schema in [
        INITIAL_SCHEMA,
        LOCAL_AUTH_SCHEMA,
        TRANSFER_SCHEMA,
        DOWNLOAD_SCHEMA,
        LIFECYCLE_SCHEMA,
    ] {
        connection.execute_batch(schema).expect("schema migration");
    }
    connection
        .execute_batch(
            "INSERT INTO workspaces (id, name, slug) VALUES ('workspace_v5', 'V5', 'v5');
             INSERT INTO api_tokens (id, name, secret_hash, scopes, workspace_id, revoked)
             VALUES
               ('token_active', 'Active', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'object:read', 'workspace_v5', 0),
               ('token_revoked', 'Revoked', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'object:read', 'workspace_v5', 1);
             PRAGMA user_version = 5;",
        )
        .expect("v5 fixture");
    drop(connection);

    let repository = SqliteRepository::open(&path).expect("migrated repository");
    assert_eq!(repository.schema_version().expect("schema version"), 16);
    let tokens = repository.list_api_tokens().expect("migrated tokens");
    let active = tokens
        .iter()
        .find(|token| token.id == "token_active")
        .expect("active token");
    assert_eq!(active.token_prefix, "legacy");
    assert_eq!(active.created_at_ms, 0);
    assert_eq!(active.expires_at_ms, i64::MAX as u64);
    assert_eq!(active.last_used_at_ms, None);
    assert_eq!(active.revoked_at_ms, None);
    let revoked = tokens
        .iter()
        .find(|token| token.id == "token_revoked")
        .expect("revoked token");
    assert_eq!(revoked.token_prefix, "legacy");
    assert_eq!(revoked.revoked_at_ms, Some(0));
    assert_eq!(
        repository.authenticate_api_token(&active.secret_hash, 1),
        Ok(blobyard_contract::LocalApiTokenRecord {
            last_used_at_ms: Some(1),
            ..active.clone()
        })
    );
    assert_eq!(
        repository.authenticate_api_token(&revoked.secret_hash, 1),
        Err(blobyard_contract::RepositoryError::NotFound)
    );
}

#[test]
fn sqlite_migrates_completed_v3_objects_without_losing_download_metadata() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("v3.sqlite3");
    let connection = rusqlite::Connection::open(&path).expect("sqlite");
    connection.execute_batch(INITIAL_SCHEMA).expect("v1 schema");
    connection
        .execute_batch(LOCAL_AUTH_SCHEMA)
        .expect("v2 schema");
    connection
        .execute_batch(TRANSFER_SCHEMA)
        .expect("v3 schema");
    connection
        .execute_batch(
            "INSERT INTO workspaces (id, name, slug) VALUES ('workspace_v3', 'V3', 'v3');
             INSERT INTO projects (id, workspace_id, name, slug) VALUES ('project_v3', 'workspace_v3', 'V3', 'v3');
             INSERT INTO object_versions (id, project_id, object_path, version, storage_key, state, size, checksum)
             VALUES ('version_v3', 'project_v3', 'archive.bin', 1, 'objects/version_v3', 'complete', 4, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
             INSERT INTO upload_reservations (id, version_id, filename, content_type, expected_size, expected_checksum, capability_hash, expires_at_ms, state, received_size, received_checksum)
             VALUES ('upload_v3', 'version_v3', 'archive.bin', 'application/octet-stream', 4, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 1000, 'complete', 4, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
             PRAGMA user_version = 3;",
        )
        .expect("v3 fixture");
    drop(connection);

    let repository = SqliteRepository::open(&path).expect("migrated repository");
    assert_eq!(repository.schema_version().expect("schema version"), 16);
    let objects = repository
        .list_stored_objects("project_v3", None, true)
        .expect("migrated objects");
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].version.id, "version_v3");
    assert_eq!(objects[0].version.created_at_ms, 0);
    assert_eq!(
        objects[0].version.source,
        blobyard_contract::ObjectSource::Cli
    );
    assert_eq!(objects[0].version.git_repository, None);
    assert_eq!(objects[0].version.git_commit, None);
    assert_eq!(objects[0].version.git_branch, None);
    repository
        .issue_download(&NewDownloadGrant {
            version_id: "version_v3".to_owned(),
            capability_hash: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                .to_owned(),
            expires_at_ms: 2000,
        })
        .expect("download grant");
    assert_eq!(
        repository
            .download_by_capability(
                "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                1999,
            )
            .expect("download object")
            .version
            .id,
        "version_v3"
    );
}

#[test]
fn sqlite_migrates_v4_objects_into_lifecycle_schema() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("v4.sqlite3");
    let connection = rusqlite::Connection::open(&path).expect("sqlite");
    for schema in [
        INITIAL_SCHEMA,
        LOCAL_AUTH_SCHEMA,
        TRANSFER_SCHEMA,
        DOWNLOAD_SCHEMA,
    ] {
        connection.execute_batch(schema).expect("schema migration");
    }
    connection
        .execute_batch(
            "INSERT INTO workspaces (id, name, slug) VALUES ('workspace_v4', 'V4', 'v4');
             INSERT INTO projects (id, workspace_id, name, slug) VALUES ('project_v4', 'workspace_v4', 'V4', 'v4');
             INSERT INTO object_versions (id, project_id, object_path, version, storage_key, state, size, checksum, created_at_ms)
             VALUES ('version_v4', 'project_v4', 'archive.bin', 1, 'objects/version_v4', 'complete', 4, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 10);
             PRAGMA user_version = 4;",
        )
        .expect("v4 fixture");
    drop(connection);

    let repository = SqliteRepository::open(&path).expect("migrated repository");
    assert_eq!(repository.schema_version().expect("schema version"), 16);
    let object = repository
        .object_version("version_v4")
        .expect("migrated object");
    assert_eq!(object.source, blobyard_contract::ObjectSource::Cli);
    assert_eq!(object.git_repository, None);
    assert_eq!(object.git_commit, None);
    assert_eq!(object.git_branch, None);
}
