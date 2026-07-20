use super::{Corruption, FaultingRepository};
use blobyard_contract::{
    InboxRateResult, InboxRecord, InboxRepository, NewAuditEvent, NewInbox, NewInboxUpload,
    NewUploadReservation, ObjectVersionRecord, RepositoryError, UploadReservationRecord,
};

fn corrupt_expiry(records: &mut [InboxRecord]) {
    for record in records {
        record.expires_at_ms = u64::MAX;
    }
}

impl InboxRepository for FaultingRepository {
    fn create_inbox(
        &self,
        inbox: &NewInbox,
        event: &NewAuditEvent,
    ) -> Result<InboxRecord, RepositoryError> {
        self.check()?;
        self.inner.create_inbox(inbox, event)
    }

    fn list_inboxes(&self, project_id: &str) -> Result<Vec<InboxRecord>, RepositoryError> {
        self.check()?;
        self.inner.list_inboxes(project_id).map(|mut records| {
            if matches!(self.corruption, Some(Corruption::InboxExpiry)) {
                corrupt_expiry(&mut records);
            }
            records
        })
    }

    fn inbox_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<InboxRecord, RepositoryError> {
        self.check()?;
        self.inner.inbox_by_capability(capability_hash, now_ms)
    }

    fn consume_inbox_rate(
        &self,
        rate_key: &str,
        window_ms: u64,
        limit: u32,
        now_ms: u64,
    ) -> Result<InboxRateResult, RepositoryError> {
        self.check()?;
        self.inner
            .consume_inbox_rate(rate_key, window_ms, limit, now_ms)
    }

    fn reserve_inbox_upload(
        &self,
        inbox_upload: &NewInboxUpload,
        reservation: &NewUploadReservation,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        self.check()?;
        self.inner.reserve_inbox_upload(inbox_upload, reservation)
    }

    fn inbox_upload_by_id(
        &self,
        capability_hash: &str,
        upload_id: &str,
        now_ms: u64,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        self.check()?;
        self.inner
            .inbox_upload_by_id(capability_hash, upload_id, now_ms)
    }

    fn complete_inbox_upload(
        &self,
        capability_hash: &str,
        upload_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<ObjectVersionRecord, RepositoryError> {
        self.check()?;
        self.inner
            .complete_inbox_upload(capability_hash, upload_id, now_ms, event)
    }

    fn abort_inbox_upload(
        &self,
        capability_hash: &str,
        upload_id: &str,
        now_ms: u64,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        self.check()?;
        self.inner
            .abort_inbox_upload(capability_hash, upload_id, now_ms)
    }

    fn revoke_inbox(
        &self,
        inbox_id: &str,
        workspace_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        self.check()?;
        self.inner
            .revoke_inbox(inbox_id, workspace_id, revoked_at_ms, event)
    }
}
