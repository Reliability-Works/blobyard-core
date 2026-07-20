#![allow(
    dead_code,
    reason = "shared across lifecycle integration test binaries"
)]

use blobyard_contract::{
    AuditValue, MetadataRepository, NewAuditEvent, NewObjectDeletion, NewObjectVersion,
    ObjectDeletionTarget, ProjectRecord, WorkspaceRecord,
};
use blobyard_core::Slug;
use blobyard_repository_sqlite::SqliteRepository;

pub(crate) struct Fixture {
    pub(crate) temporary: tempfile::TempDir,
    pub(crate) path: std::path::PathBuf,
    pub(crate) repository: SqliteRepository,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("metadata.sqlite3");
        let repository = SqliteRepository::open(&path).expect("repository");
        repository
            .create_workspace(&workspace_record())
            .expect("workspace");
        repository
            .create_project(&project_record())
            .expect("project");
        Self {
            temporary,
            path,
            repository,
        }
    }

    pub(crate) fn store_complete(
        &self,
        id: &str,
        path: &str,
        version: u64,
        created_at_ms: u64,
        branch: Option<&str>,
    ) {
        self.repository
            .reserve_object_version(&version_record(id, path, version))
            .expect("reserved object");
        self.repository
            .complete_object_version(
                id,
                5,
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            )
            .expect("completed object");
        let connection = rusqlite::Connection::open(&self.path).expect("fixture database");
        connection
            .execute(
                "UPDATE object_versions SET created_at_ms = ?2, git_branch = ?3 WHERE id = ?1",
                rusqlite::params![
                    id,
                    i64::try_from(created_at_ms).expect("fixture time"),
                    branch
                ],
            )
            .expect("object provenance");
    }

    pub(crate) fn store_pending(&self, id: &str, path: &str, version: u64) {
        self.repository
            .reserve_object_version(&version_record(id, path, version))
            .expect("pending object");
    }

    pub(crate) fn store_aborted(&self, id: &str, path: &str, version: u64) {
        self.store_pending(id, path, version);
        self.repository
            .abort_object_version(id)
            .expect("aborted object");
    }
}

pub(crate) fn deletion(id: &str, path: &str, version: Option<u64>) -> NewObjectDeletion {
    NewObjectDeletion {
        id: id.to_owned(),
        target: ObjectDeletionTarget {
            project_id: "project_fixture".to_owned(),
            object_path: path.to_owned(),
            version,
        },
        actor: "token_fixture".to_owned(),
        request_id: "request_delete".to_owned(),
        created_at_ms: 1,
    }
}

pub(crate) fn event(id: &str, action: &str, request_id: &str, created_at_ms: u64) -> NewAuditEvent {
    NewAuditEvent {
        id: id.to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: if action == "retention.enforced" {
            "system:retention".to_owned()
        } else {
            "token_fixture".to_owned()
        },
        action: action.to_owned(),
        request_id: request_id.to_owned(),
        target_type: "fixture".to_owned(),
        metadata: vec![("fixture".to_owned(), AuditValue::Boolean(true))],
        created_at_ms,
    }
}

fn version_record(id: &str, path: &str, version: u64) -> NewObjectVersion {
    NewObjectVersion {
        id: id.to_owned(),
        project_id: "project_fixture".to_owned(),
        object_path: path.to_owned(),
        version,
        storage_key: format!("objects/{id}"),
        source: blobyard_contract::ObjectSource::Cli,
        git_repository: None,
        git_commit: None,
        git_branch: None,
    }
}

fn workspace_record() -> WorkspaceRecord {
    WorkspaceRecord {
        id: "workspace_fixture".to_owned(),
        name: "Fixture".to_owned(),
        slug: slug("fixture"),
    }
}

fn project_record() -> ProjectRecord {
    ProjectRecord {
        id: "project_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        name: "Fixture".to_owned(),
        slug: slug("fixture"),
    }
}

fn slug(value: &str) -> Slug {
    Slug::new(value.to_owned()).expect("valid slug")
}
