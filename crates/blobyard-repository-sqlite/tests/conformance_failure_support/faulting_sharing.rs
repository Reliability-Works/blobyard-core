use super::Faulting;
use blobyard_contract::{
    NewAuditEvent, NewDownloadGrant, NewShare, RepositoryError, ShareRecord, ShareTarget,
    SharingRepository,
};

impl<T: SharingRepository> SharingRepository for Faulting<'_, T> {
    fn create_share(
        &self,
        share: &NewShare,
        event: &NewAuditEvent,
    ) -> Result<ShareRecord, RepositoryError> {
        self.check()?;
        self.inner.create_share(share, event)
    }

    fn list_shares(&self, workspace_id: &str) -> Result<Vec<ShareRecord>, RepositoryError> {
        self.check()?;
        self.inner.list_shares(workspace_id)
    }

    fn share_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<ShareTarget, RepositoryError> {
        self.check()?;
        self.inner.share_by_capability(capability_hash, now_ms)
    }

    fn issue_share_download(
        &self,
        capability_hash: &str,
        now_ms: u64,
        grant: &NewDownloadGrant,
        event: &NewAuditEvent,
    ) -> Result<ShareTarget, RepositoryError> {
        self.check()?;
        self.inner
            .issue_share_download(capability_hash, now_ms, grant, event)
    }

    fn revoke_share(
        &self,
        share_id: &str,
        workspace_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        self.check()?;
        self.inner
            .revoke_share(share_id, workspace_id, revoked_at_ms, event)
    }
}
