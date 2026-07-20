use super::{Corrupting, Corruption};
use blobyard_contract::{
    NewAuditEvent, NewPreview, PreviewRecord, PreviewRepository, PreviewStatus, PreviewTarget,
    RepositoryError,
};

impl<T: PreviewRepository> PreviewRepository for Corrupting<'_, T> {
    fn create_preview(
        &self,
        preview: &NewPreview,
        event: &NewAuditEvent,
    ) -> Result<PreviewRecord, RepositoryError> {
        self.inner.create_preview(preview, event).map(|mut record| {
            if matches!(self.corruption, Corruption::PreviewCreatedRecord) {
                record.status = PreviewStatus::Revoked;
            }
            record
        })
    }

    fn list_previews(&self, project_id: &str) -> Result<Vec<PreviewRecord>, RepositoryError> {
        self.inner.list_previews(project_id).map(|mut records| {
            match self.corruption {
                Corruption::PreviewInitialList if records.is_empty() => {
                    records.push(PreviewRecord {
                        id: "unexpected".to_owned(),
                        workspace_id: "workspace_fixture".to_owned(),
                        project_id: project_id.to_owned(),
                        expires_at_ms: 1,
                        status: PreviewStatus::Active,
                        created_at_ms: 0,
                        revoked_at_ms: None,
                    });
                }
                Corruption::PreviewList if !records.is_empty() => records.clear(),
                _ => {}
            }
            records
        })
    }

    fn preview_by_id(&self, preview_id: &str) -> Result<PreviewRecord, RepositoryError> {
        self.inner.preview_by_id(preview_id)
    }

    fn preview_file_by_capability(
        &self,
        capability_hash: &str,
        normalized_path: &str,
        now_ms: u64,
    ) -> Result<PreviewTarget, RepositoryError> {
        let result =
            self.inner
                .preview_file_by_capability(capability_hash, normalized_path, now_ms);
        match self.corruption {
            Corruption::PreviewResolvedTarget
                if normalized_path == "index.html" && now_ms == 7_999 =>
            {
                result.map(|mut target| {
                    "wrong.html".clone_into(&mut target.normalized_path);
                    target
                })
            }
            Corruption::PreviewMissingResolution if normalized_path == "missing" => {
                result.map_err(|_error| RepositoryError::Unavailable)
            }
            Corruption::PreviewExpiredResolution if now_ms == 8_000 => {
                result.map_err(|_error| RepositoryError::Unavailable)
            }
            Corruption::PreviewRevokedResolution if now_ms == 7_501 => {
                result.map_err(|_error| RepositoryError::Unavailable)
            }
            _ => result,
        }
    }

    fn revoke_preview(
        &self,
        preview_id: &str,
        workspace_id: &str,
        project_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        self.inner
            .revoke_preview(preview_id, workspace_id, project_id, revoked_at_ms, event)
            .map(|revoked| match self.corruption {
                Corruption::PreviewFirstRevoke => false,
                Corruption::PreviewSecondRevoke => true,
                _ => revoked,
            })
    }
}
