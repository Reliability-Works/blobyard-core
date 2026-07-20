#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use blobyard_contract::{
    LifecycleRepository, MetadataRepository, MetadataRepositoryInventory, ObjectSource,
    ProjectRecord, SharingRepository, TransferRepository, UploadState, WorkspaceRecord,
};
use blobyard_core::Slug;

pub(super) fn repository() -> (tempfile::TempDir, SqliteRepository) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let database = temporary.path().join("metadata.sqlite3");
    let repository = SqliteRepository::open(&database).expect("migration repository");
    (temporary, repository)
}

pub(super) fn snapshot() -> MigrationSnapshot {
    MigrationSnapshot {
        workspaces: vec![WorkspaceRecord {
            id: "workspace_default".to_owned(),
            name: "Migrated Workspace".to_owned(),
            slug: Slug::new("migrated").expect("workspace slug"),
        }],
        projects: vec![ProjectRecord {
            id: "project_migrated".to_owned(),
            workspace_id: "workspace_default".to_owned(),
            name: "Migrated Project".to_owned(),
            slug: Slug::new("project").expect("project slug"),
        }],
        objects: vec![MigrationObjectRecord {
            id: "version_migrated".to_owned(),
            project_id: "project_migrated".to_owned(),
            object_path: "releases/app.zip".to_owned(),
            version: 7,
            storage_key: "objects/migrated/version-7".to_owned(),
            size: 3,
            checksum: "a".repeat(64),
            created_at_ms: 1_000,
            source: ObjectSource::Ci,
            git_repository: Some("Reliability-Works/example".to_owned()),
            git_commit: Some("b".repeat(40)),
            git_branch: Some("main".to_owned()),
            filename: "app.zip".to_owned(),
            content_type: "application/zip".to_owned(),
        }],
        shares: vec![MigrationShareRecord {
            id: "share_migrated".to_owned(),
            workspace_id: "workspace_default".to_owned(),
            version_id: "version_migrated".to_owned(),
            capability_hash: "c".repeat(64),
            expires_at_ms: 3_000,
            status: ShareStatus::Active,
            consumed_count: 1,
            maximum_downloads: Some(3),
            created_at_ms: 2_000,
            revoked_at_ms: None,
        }],
        retention: vec![MigrationRetentionRecord {
            project_id: "project_migrated".to_owned(),
            keep_latest: 4,
            path_glob: Some("releases/**".to_owned()),
            branch_glob: Some("main".to_owned()),
            enabled: true,
            created_at_ms: 2_100,
            updated_at_ms: 2_200,
        }],
    }
}

#[test]
fn import_preserves_metadata_and_completed_transfer_contracts() {
    let (_temporary, repository) = repository();
    let snapshot = snapshot();

    repository
        .import_migration(&snapshot)
        .expect("migration import");

    assert_workspace_and_project(&repository, &snapshot);
    assert_version(&repository, &snapshot);
    assert_object_metadata(&repository);
    assert_share(&repository);
    assert_retention(&repository);
}

fn assert_workspace_and_project(repository: &SqliteRepository, snapshot: &MigrationSnapshot) {
    assert_eq!(
        repository.list_workspaces().expect("workspaces"),
        snapshot.workspaces
    );
    assert_eq!(
        repository
            .list_projects("workspace_default")
            .expect("projects"),
        snapshot.projects
    );
}

fn assert_version(repository: &SqliteRepository, snapshot: &MigrationSnapshot) {
    let version = repository
        .object_version("version_migrated")
        .expect("object version");
    assert_eq!(version.id, snapshot.objects[0].id);
    assert_eq!(version.version, 7);
    assert_eq!(version.state, UploadState::Complete);
    assert_eq!(version.size, Some(3));
    assert_eq!(version.checksum, Some("a".repeat(64)));
    assert_eq!(version.created_at_ms, 1_000);
    assert_eq!(version.source, ObjectSource::Ci);
    assert_eq!(version.git_repository, snapshot.objects[0].git_repository);
    assert_eq!(version.git_commit, snapshot.objects[0].git_commit);
    assert_eq!(version.git_branch, snapshot.objects[0].git_branch);
    assert_eq!(
        repository.list_object_versions().expect("inventory"),
        vec![version]
    );
}

fn assert_object_metadata(repository: &SqliteRepository) {
    let objects = repository
        .list_stored_objects("project_migrated", None, true)
        .expect("stored objects");
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].filename, "app.zip");
    assert_eq!(objects[0].content_type, "application/zip");
}

fn assert_share(repository: &SqliteRepository) {
    let shares = repository.list_shares("workspace_default").expect("shares");
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].id, "share_migrated");
    assert_eq!(shares[0].version_id.as_deref(), Some("version_migrated"));
    assert_eq!(shares[0].consumed_count, 1);
    assert_eq!(shares[0].maximum_downloads, Some(3));
    assert_eq!(shares[0].status, ShareStatus::Active);
    assert_eq!(shares[0].created_at_ms, 2_000);
}

fn assert_retention(repository: &SqliteRepository) {
    let retention = repository
        .retention_policy("project_migrated")
        .expect("retention policy");
    assert_eq!(retention.keep_latest, 4);
    assert_eq!(retention.path_glob.as_deref(), Some("releases/**"));
    assert_eq!(retention.branch_glob.as_deref(), Some("main"));
    assert_eq!(retention.created_at_ms, 2_100);
    assert_eq!(retention.updated_at_ms, 2_200);
}

#[test]
fn occupied_repository_rejects_import_without_mutation() {
    let (_temporary, repository) = repository();
    let existing = WorkspaceRecord {
        id: "existing".to_owned(),
        name: "Existing".to_owned(),
        slug: Slug::new("existing").expect("slug"),
    };
    repository
        .create_workspace(&existing)
        .expect("existing workspace");

    assert_eq!(
        repository.import_migration(&snapshot()),
        Err(RepositoryError::Conflict)
    );
    assert_eq!(
        repository.list_workspaces().expect("workspaces"),
        vec![existing]
    );
    assert!(
        repository
            .list_object_versions()
            .expect("objects")
            .is_empty()
    );
}

#[test]
fn relational_failure_rolls_back_every_insert() {
    let (_temporary, repository) = repository();
    let mut invalid = snapshot();
    invalid.retention[0].project_id = "missing_project".to_owned();

    assert_eq!(
        repository.import_migration(&invalid),
        Err(RepositoryError::Conflict)
    );
    assert!(repository.list_workspaces().expect("workspaces").is_empty());
    assert!(
        repository
            .list_object_versions()
            .expect("objects")
            .is_empty()
    );
}

#[test]
fn snapshot_validation_rejects_invalid_object_share_and_retention() {
    let mutations: [fn(&mut MigrationSnapshot); 7] = [
        |value| value.workspaces.clear(),
        |value| value.workspaces[0].id = "not_default".to_owned(),
        |value| value.objects[0].version = 0,
        |value| value.objects[0].checksum = "invalid".to_owned(),
        |value| value.shares[0].expires_at_ms = value.shares[0].created_at_ms,
        |value| value.shares[0].status = ShareStatus::Exhausted,
        |value| value.retention[0].keep_latest = 0,
    ];
    assert_invalid_imports(&mutations);
}

#[test]
fn disabled_retention_is_preserved_but_not_exposed_as_active() {
    let (_temporary, repository) = repository();
    let mut snapshot = snapshot();
    snapshot.retention[0].enabled = false;

    repository
        .import_migration(&snapshot)
        .expect("migration import");

    assert_eq!(
        repository.retention_policy("project_migrated"),
        Err(RepositoryError::NotFound)
    );
    let enabled: bool = repository
        .test_connection()
        .expect("connection")
        .query_row(
            "SELECT enabled FROM retention_policies WHERE project_id = 'project_migrated'",
            [],
            |row| row.get(0),
        )
        .expect("enabled flag");
    assert!(!enabled);
}

#[test]
fn share_validation_accepts_exact_revoked_exhausted_and_unbounded_states() {
    let mut revoked = snapshot().shares.remove(0);
    revoked.status = ShareStatus::Revoked;
    revoked.revoked_at_ms = Some(revoked.created_at_ms);
    assert!(validate_share(&revoked).is_ok());

    let mut exhausted = revoked;
    exhausted.status = ShareStatus::Exhausted;
    exhausted.revoked_at_ms = None;
    exhausted.consumed_count = 3;
    assert!(validate_share(&exhausted).is_ok());

    let mut unbounded = exhausted;
    unbounded.status = ShareStatus::Active;
    unbounded.maximum_downloads = None;
    assert!(validate_share(&unbounded).is_ok());
}

#[test]
fn share_validation_rejects_each_counter_status_and_revocation_boundary() {
    let mutations: [fn(&mut MigrationShareRecord); 6] = [
        |value| value.maximum_downloads = Some(0),
        |value| value.consumed_count = 4,
        |value| value.status = ShareStatus::Exhausted,
        |value| value.revoked_at_ms = Some(value.created_at_ms),
        |value| {
            value.status = ShareStatus::Revoked;
            value.revoked_at_ms = Some(value.created_at_ms - 1);
        },
        |value| value.capability_hash = "invalid".to_owned(),
    ];
    for mutate in mutations {
        let mut share = snapshot().shares.remove(0);
        mutate(&mut share);
        assert_eq!(validate_share(&share), Err(RepositoryError::InvalidInput));
    }
}

#[test]
fn import_rejects_each_integer_that_sqlite_cannot_represent() {
    let mutations: [fn(&mut MigrationSnapshot); 8] = [
        |value| value.objects[0].version = u64::MAX,
        |value| value.objects[0].size = u64::MAX,
        |value| value.objects[0].created_at_ms = u64::MAX,
        |value| value.shares[0].expires_at_ms = u64::MAX,
        |value| value.shares[0].consumed_count = u64::MAX,
        |value| value.shares[0].maximum_downloads = Some(u64::MAX),
        |value| value.shares[0].created_at_ms = u64::MAX,
        |value| {
            value.shares[0].status = ShareStatus::Revoked;
            value.shares[0].revoked_at_ms = Some(u64::MAX);
        },
    ];
    assert_invalid_imports(&mutations);
}

fn assert_invalid_imports(mutations: &[fn(&mut MigrationSnapshot)]) {
    for mutate in mutations {
        let (_temporary, repository) = repository();
        let mut invalid = snapshot();
        mutate(&mut invalid);
        assert_eq!(
            repository.import_migration(&invalid),
            Err(RepositoryError::InvalidInput)
        );
        assert!(repository.list_workspaces().expect("workspaces").is_empty());
    }
}
