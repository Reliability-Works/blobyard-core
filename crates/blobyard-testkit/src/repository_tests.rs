use super::{NamespaceFixtures, repository_conformance_with};
use blobyard_contract::{
    MetadataRepository, NewObjectVersion, ObjectVersionRecord, ProjectRecord, RepositoryError,
    WorkspaceRecord,
};
use blobyard_core::Slug;

struct UnusedRepository;

impl MetadataRepository for UnusedRepository {
    fn schema_version(&self) -> Result<u32, RepositoryError> {
        unreachable!()
    }

    fn create_workspace(&self, _workspace: &WorkspaceRecord) -> Result<(), RepositoryError> {
        unreachable!()
    }

    fn list_workspaces(&self) -> Result<Vec<WorkspaceRecord>, RepositoryError> {
        unreachable!()
    }

    fn workspace_by_slug(&self, _slug: &Slug) -> Result<WorkspaceRecord, RepositoryError> {
        unreachable!()
    }

    fn rename_workspace(
        &self,
        _workspace: &WorkspaceRecord,
        _event: &blobyard_contract::NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        unreachable!()
    }

    fn create_project(&self, _project: &ProjectRecord) -> Result<(), RepositoryError> {
        unreachable!()
    }

    fn list_projects(&self, _workspace_id: &str) -> Result<Vec<ProjectRecord>, RepositoryError> {
        unreachable!()
    }

    fn project_by_slug(
        &self,
        _workspace_id: &str,
        _slug: &Slug,
    ) -> Result<ProjectRecord, RepositoryError> {
        unreachable!()
    }

    fn reserve_object_version(&self, _version: &NewObjectVersion) -> Result<(), RepositoryError> {
        unreachable!()
    }

    fn complete_object_version(
        &self,
        _id: &str,
        _size: u64,
        _checksum: &str,
    ) -> Result<(), RepositoryError> {
        unreachable!()
    }

    fn abort_object_version(&self, _id: &str) -> Result<(), RepositoryError> {
        unreachable!()
    }

    fn object_version(&self, _id: &str) -> Result<ObjectVersionRecord, RepositoryError> {
        unreachable!()
    }
}

#[test]
fn conformance_rejects_invalid_namespace_fixtures_before_adapter_calls() {
    for fixtures in [
        NamespaceFixtures {
            workspace: "invalid workspace",
            ..NamespaceFixtures::valid()
        },
        NamespaceFixtures {
            renamed_workspace: "invalid renamed workspace",
            ..NamespaceFixtures::valid()
        },
        NamespaceFixtures {
            project: "invalid project",
            ..NamespaceFixtures::valid()
        },
    ] {
        assert_eq!(
            repository_conformance_with(&UnusedRepository, &fixtures),
            Err(RepositoryError::Unavailable)
        );
    }
}
