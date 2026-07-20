use super::{Corrupting, Corruption};
use blobyard_contract::{
    InboxRateResult, InboxRecord, InboxRepository, NewAuditEvent, NewInbox, NewInboxUpload,
    NewUploadReservation, ObjectVersionRecord, RepositoryError, ReservationState,
    UploadReservationRecord, UploadState,
};
use std::sync::atomic::Ordering;

impl<T: InboxRepository> InboxRepository for Corrupting<'_, T> {
    fn create_inbox(
        &self,
        inbox: &NewInbox,
        event: &NewAuditEvent,
    ) -> Result<InboxRecord, RepositoryError> {
        self.inner.create_inbox(inbox, event).map(|mut record| {
            if matches!(self.corruption, Corruption::InboxCreatedRecord) {
                record.current_files = 1;
            }
            record
        })
    }

    fn list_inboxes(&self, project_id: &str) -> Result<Vec<InboxRecord>, RepositoryError> {
        self.inner.list_inboxes(project_id).map(|mut records| {
            let call = self.inbox_list_calls.fetch_add(1, Ordering::Relaxed) + 1;
            match self.corruption {
                Corruption::InboxList => records.clear(),
                Corruption::InboxReservedList if call == 2 => records.clear(),
                Corruption::InboxReservedCounters
                    if records
                        .first()
                        .is_some_and(|record| record.reserved_files == 1) =>
                {
                    records[0].reserved_files = 0;
                }
                Corruption::InboxCompletedList if call == 3 => records.clear(),
                Corruption::InboxCompletedCounters
                    if records
                        .first()
                        .is_some_and(|record| record.current_files == 1) =>
                {
                    records[0].current_files = 0;
                }
                Corruption::InboxReleasedList if call == 4 => records.clear(),
                Corruption::InboxReleasedCounters if call == 4 => {
                    records[0].reserved_bytes = 1;
                }
                _ => {}
            }
            records
        })
    }

    fn inbox_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<InboxRecord, RepositoryError> {
        let result = self.inner.inbox_by_capability(capability_hash, now_ms);
        match self.corruption {
            Corruption::InboxResolvedRecord => result.map(|mut record| {
                "wrong-inbox".clone_into(&mut record.id);
                record
            }),
            Corruption::InboxExpiryResult if now_ms == 5_000 => {
                result.map_err(|_error| RepositoryError::Unavailable)
            }
            Corruption::InboxRevokedResolve if now_ms == 1_401 => {
                result.map_err(|_error| RepositoryError::Unavailable)
            }
            _ => result,
        }
    }

    fn consume_inbox_rate(
        &self,
        rate_key: &str,
        window_ms: u64,
        limit: u32,
        now_ms: u64,
    ) -> Result<InboxRateResult, RepositoryError> {
        let result = self
            .inner
            .consume_inbox_rate(rate_key, window_ms, limit, now_ms)?;
        Ok(match (self.corruption, now_ms) {
            (Corruption::InboxRateLimited, 1_500) => InboxRateResult::Allowed,
            (Corruption::InboxRateAllowed, 1_000) | (Corruption::InboxRateReset, 2_000) => {
                InboxRateResult::Limited {
                    retry_after_seconds: 1,
                }
            }
            _ => result,
        })
    }

    fn reserve_inbox_upload(
        &self,
        inbox_upload: &NewInboxUpload,
        reservation: &NewUploadReservation,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        let result = self.inner.reserve_inbox_upload(inbox_upload, reservation);
        if matches!(self.corruption, Corruption::InboxCapacityResult)
            && reservation.id == "inbox_upload_over_capacity"
        {
            return result.map_err(|_error| RepositoryError::Unavailable);
        }
        result.map(|mut record| {
            if matches!(self.corruption, Corruption::InboxReservedRecord)
                && reservation.id == "inbox_upload_complete"
            {
                record.version.source = blobyard_contract::ObjectSource::Cli;
            }
            record
        })
    }

    fn inbox_upload_by_id(
        &self,
        capability_hash: &str,
        upload_id: &str,
        now_ms: u64,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        self.inner
            .inbox_upload_by_id(capability_hash, upload_id, now_ms)
            .map(|mut record| {
                if matches!(self.corruption, Corruption::InboxAbortStored)
                    && upload_id == "inbox_upload_abort"
                    && record.state == ReservationState::Aborted
                {
                    record.state = ReservationState::Requested;
                }
                record
            })
    }

    fn complete_inbox_upload(
        &self,
        capability_hash: &str,
        upload_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<ObjectVersionRecord, RepositoryError> {
        self.inner
            .complete_inbox_upload(capability_hash, upload_id, now_ms, event)
            .map(|mut record| {
                if matches!(self.corruption, Corruption::InboxCompletedRecord) {
                    record.state = UploadState::Pending;
                }
                record
            })
    }

    fn abort_inbox_upload(
        &self,
        capability_hash: &str,
        upload_id: &str,
        now_ms: u64,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        self.inner
            .abort_inbox_upload(capability_hash, upload_id, now_ms)
            .map(|mut record| {
                if matches!(self.corruption, Corruption::InboxAbortPrior) {
                    record.state = ReservationState::Aborted;
                }
                record
            })
    }

    fn revoke_inbox(
        &self,
        inbox_id: &str,
        workspace_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        self.inner
            .revoke_inbox(inbox_id, workspace_id, revoked_at_ms, event)
            .map(|revoked| match self.corruption {
                Corruption::InboxFirstRevoke if revoked_at_ms == 1_400 => false,
                Corruption::InboxSecondRevoke if revoked_at_ms == 1_401 => true,
                _ => revoked,
            })
    }
}
