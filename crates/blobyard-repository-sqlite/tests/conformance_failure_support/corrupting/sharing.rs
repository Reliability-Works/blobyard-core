use super::{Corrupting, Corruption};
use blobyard_contract::{
    NewAuditEvent, NewDownloadGrant, NewShare, RepositoryError, ShareRecord, ShareStatus,
    ShareTarget, SharingRepository,
};

impl<T: SharingRepository> SharingRepository for Corrupting<'_, T> {
    fn create_share(
        &self,
        share: &NewShare,
        event: &NewAuditEvent,
    ) -> Result<ShareRecord, RepositoryError> {
        self.inner.create_share(share, event).map(|mut record| {
            if matches!(self.corruption, Corruption::ShareCreatedRecord) {
                record.consumed_count = 1;
            }
            record
        })
    }

    fn list_shares(&self, workspace_id: &str) -> Result<Vec<ShareRecord>, RepositoryError> {
        self.inner.list_shares(workspace_id).map(|mut records| {
            match self.corruption {
                Corruption::ShareList
                    if records
                        .first()
                        .is_some_and(|record| record.status == ShareStatus::Active) =>
                {
                    records.clear();
                }
                Corruption::ShareFinalRecord
                    if records
                        .first()
                        .is_some_and(|record| record.status == ShareStatus::Revoked) =>
                {
                    records[0].status = ShareStatus::Active;
                }
                Corruption::ShareFinalList
                    if records
                        .first()
                        .is_some_and(|record| record.status == ShareStatus::Revoked) =>
                {
                    records.clear();
                }
                _ => {}
            }
            records
        })
    }

    fn share_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<ShareTarget, RepositoryError> {
        self.inner
            .share_by_capability(capability_hash, now_ms)
            .map(|mut target| {
                if matches!(self.corruption, Corruption::ShareResolvedTarget) {
                    "wrong-version".clone_into(&mut target.object.version.id);
                }
                target
            })
    }

    fn issue_share_download(
        &self,
        capability_hash: &str,
        now_ms: u64,
        grant: &NewDownloadGrant,
        event: &NewAuditEvent,
    ) -> Result<ShareTarget, RepositoryError> {
        self.inner
            .issue_share_download(capability_hash, now_ms, grant, event)
            .map(|mut target| {
                if matches!(self.corruption, Corruption::ShareIssuedTarget) {
                    target.share.status = ShareStatus::Active;
                }
                target
            })
    }

    fn revoke_share(
        &self,
        share_id: &str,
        workspace_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        self.inner
            .revoke_share(share_id, workspace_id, revoked_at_ms, event)
            .map(|revoked| match self.corruption {
                Corruption::ShareFirstRevoke if revoked_at_ms == 1_003 => false,
                Corruption::ShareSecondRevoke if revoked_at_ms == 1_004 => true,
                _ => revoked,
            })
    }
}
