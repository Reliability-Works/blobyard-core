use super::{Corrupting, Corruption};
use blobyard_contract::{
    MetadataRepository, NewObjectVersion, ObjectVersionRecord, ProjectRecord, RepositoryError,
    UploadState, WorkspaceRecord,
};
use blobyard_core::Slug;

impl<T: MetadataRepository> MetadataRepository for Corrupting<'_, T> {
    fn schema_version(&self) -> Result<u32, RepositoryError> {
        self.inner
            .schema_version()
            .map(|value| match self.corruption {
                Corruption::SchemaVersion => value + 1,
                _ => value,
            })
    }

    fn create_workspace(&self, value: &WorkspaceRecord) -> Result<(), RepositoryError> {
        self.inner.create_workspace(value)
    }

    fn list_workspaces(&self) -> Result<Vec<WorkspaceRecord>, RepositoryError> {
        self.inner.list_workspaces().map(|mut values| {
            let renamed = values
                .first()
                .is_some_and(|workspace| workspace.name == "Renamed workspace");
            if matches!(self.corruption, Corruption::WorkspaceList)
                || matches!(self.corruption, Corruption::RenamedWorkspaceList) && renamed
            {
                values.clear();
            }
            values
        })
    }

    fn workspace_by_slug(&self, slug: &Slug) -> Result<WorkspaceRecord, RepositoryError> {
        self.inner.workspace_by_slug(slug).map(|mut value| {
            if matches!(self.corruption, Corruption::WorkspaceRecord)
                || matches!(self.corruption, Corruption::RenamedWorkspaceRecord)
                    && slug.as_str() == "renamed"
            {
                value.name.push_str(" changed");
            }
            value
        })
    }

    fn rename_workspace(
        &self,
        value: &WorkspaceRecord,
        event: &blobyard_contract::NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.inner.rename_workspace(value, event)
    }

    fn create_project(&self, value: &ProjectRecord) -> Result<(), RepositoryError> {
        self.inner.create_project(value)
    }

    fn list_projects(&self, workspace_id: &str) -> Result<Vec<ProjectRecord>, RepositoryError> {
        self.inner.list_projects(workspace_id).map(|mut values| {
            if matches!(self.corruption, Corruption::ProjectList) {
                values.clear();
            }
            values
        })
    }

    fn project_by_slug(
        &self,
        workspace_id: &str,
        slug: &Slug,
    ) -> Result<ProjectRecord, RepositoryError> {
        self.inner
            .project_by_slug(workspace_id, slug)
            .map(|mut value| {
                if matches!(self.corruption, Corruption::ProjectRecord) {
                    value.name.push_str(" changed");
                }
                value
            })
    }

    fn reserve_object_version(&self, value: &NewObjectVersion) -> Result<(), RepositoryError> {
        self.inner.reserve_object_version(value)
    }

    fn complete_object_version(
        &self,
        id: &str,
        size: u64,
        checksum: &str,
    ) -> Result<(), RepositoryError> {
        self.inner.complete_object_version(id, size, checksum)
    }

    fn abort_object_version(&self, id: &str) -> Result<(), RepositoryError> {
        self.inner.abort_object_version(id)
    }

    fn object_version(&self, id: &str) -> Result<ObjectVersionRecord, RepositoryError> {
        self.inner.object_version(id).map(|mut value| {
            match self.corruption {
                Corruption::CompleteState if id == "version_pending" => {
                    value.state = UploadState::Pending;
                }
                Corruption::CompleteSize if id == "version_pending" => value.size = Some(6),
                Corruption::CompleteChecksum if id == "version_pending" => {
                    value.checksum = Some("a".repeat(64));
                }
                Corruption::AbortedState if id == "version_aborted" => {
                    value.state = UploadState::Pending;
                }
                _ => {}
            }
            value
        })
    }
}
