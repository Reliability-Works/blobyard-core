use super::Faulting;
use blobyard_contract::{
    NewAuditEvent, NewPreview, PreviewRecord, PreviewRepository, PreviewTarget, RepositoryError,
};

impl<T: PreviewRepository> PreviewRepository for Faulting<'_, T> {
    fn create_preview(
        &self,
        preview: &NewPreview,
        event: &NewAuditEvent,
    ) -> Result<PreviewRecord, RepositoryError> {
        self.check()?;
        self.inner.create_preview(preview, event)
    }

    fn list_previews(&self, project_id: &str) -> Result<Vec<PreviewRecord>, RepositoryError> {
        self.check()?;
        self.inner.list_previews(project_id)
    }

    fn preview_by_id(&self, preview_id: &str) -> Result<PreviewRecord, RepositoryError> {
        self.check()?;
        self.inner.preview_by_id(preview_id)
    }

    fn preview_file_by_capability(
        &self,
        capability_hash: &str,
        normalized_path: &str,
        now_ms: u64,
    ) -> Result<PreviewTarget, RepositoryError> {
        self.check()?;
        self.inner
            .preview_file_by_capability(capability_hash, normalized_path, now_ms)
    }

    fn revoke_preview(
        &self,
        preview_id: &str,
        workspace_id: &str,
        project_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        self.check()?;
        self.inner
            .revoke_preview(preview_id, workspace_id, project_id, revoked_at_ms, event)
    }
}
