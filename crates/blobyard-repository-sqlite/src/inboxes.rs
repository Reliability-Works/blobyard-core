use super::{
    SqliteRepository, finish_audited_change, inbox_queries, inbox_rates, inbox_uploads,
    lifecycle_audit, map_error, rows, transfer_validation,
};
use blobyard_contract::{
    InboxRateResult, InboxRecord, InboxRepository, InboxStatus, NewAuditEvent, NewInbox,
    NewInboxUpload, NewUploadReservation, ObjectVersionRecord, RepositoryError,
    UploadReservationRecord,
};
use rusqlite::params;

impl InboxRepository for SqliteRepository {
    fn create_inbox(
        &self,
        inbox: &NewInbox,
        event: &NewAuditEvent,
    ) -> Result<InboxRecord, RepositoryError> {
        let values = validate_new_inbox(inbox)?;
        validate_event(
            event,
            "inbox.created",
            &inbox.id,
            &inbox.workspace_id,
            inbox.created_at_ms,
        )?;
        self.write_transaction(|transaction| {
            let changed = transaction
                .execute(
                    "INSERT INTO inboxes (id, workspace_id, project_id, name, capability_hash, expires_at_ms, status, current_files, current_bytes, reserved_files, reserved_bytes, maximum_files, maximum_bytes, created_at_ms, revoked_at_ms) SELECT ?1, ?2, p.id, ?4, ?5, ?6, 'active', 0, 0, 0, 0, ?7, ?8, ?9, NULL FROM projects p WHERE p.id = ?3 AND p.workspace_id = ?2",
                    params![
                        inbox.id,
                        inbox.workspace_id,
                        inbox.project_id,
                        inbox.name,
                        inbox.capability_hash,
                        values.expires_at_ms,
                        values.maximum_files,
                        values.maximum_bytes,
                        values.created_at_ms,
                    ],
                )
                .map_err(map_error)?;
            if changed != 1 {
                return Err(RepositoryError::NotFound);
            }
            lifecycle_audit::insert(transaction, event)?;
            inbox_queries::by_id(transaction, &inbox.id)
        })
    }

    fn list_inboxes(&self, project_id: &str) -> Result<Vec<InboxRecord>, RepositoryError> {
        rows::validate_text(project_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {} FROM inboxes WHERE project_id = ?1 ORDER BY created_at_ms DESC, id DESC",
                inbox_queries::COLUMNS
            ))
            .map_err(map_error)?;
        let result = inbox_queries::list(&mut statement, project_id);
        drop(statement);
        drop(connection);
        result
    }

    fn inbox_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<InboxRecord, RepositoryError> {
        inbox_queries::validate_capability(capability_hash)?;
        let connection = self.connection()?;
        let result = inbox_queries::active_by_capability(
            &connection,
            capability_hash,
            transfer_validation::to_i64(now_ms)?,
        );
        drop(connection);
        result
    }

    fn consume_inbox_rate(
        &self,
        rate_key: &str,
        window_ms: u64,
        limit: u32,
        now_ms: u64,
    ) -> Result<InboxRateResult, RepositoryError> {
        self.write_transaction(|transaction| {
            inbox_rates::consume(transaction, rate_key, window_ms, limit, now_ms)
        })
    }

    fn reserve_inbox_upload(
        &self,
        inbox_upload: &NewInboxUpload,
        reservation: &NewUploadReservation,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        inbox_uploads::reserve(self, inbox_upload, reservation)
    }

    fn inbox_upload_by_id(
        &self,
        capability_hash: &str,
        upload_id: &str,
        now_ms: u64,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        inbox_queries::validate_capability(capability_hash)?;
        rows::validate_text(upload_id)?;
        let connection = self.connection()?;
        let result = inbox_queries::upload(
            &connection,
            capability_hash,
            upload_id,
            transfer_validation::to_i64(now_ms)?,
        );
        drop(connection);
        result
    }

    fn complete_inbox_upload(
        &self,
        capability_hash: &str,
        upload_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<ObjectVersionRecord, RepositoryError> {
        inbox_uploads::complete(self, capability_hash, upload_id, now_ms, event)
    }

    fn abort_inbox_upload(
        &self,
        capability_hash: &str,
        upload_id: &str,
        now_ms: u64,
    ) -> Result<UploadReservationRecord, RepositoryError> {
        inbox_uploads::abort(self, capability_hash, upload_id, now_ms)
    }

    fn revoke_inbox(
        &self,
        inbox_id: &str,
        workspace_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        rows::validate_text(inbox_id)?;
        rows::validate_text(workspace_id)?;
        self.write_transaction(|transaction| {
            let inbox = inbox_queries::by_id(transaction, inbox_id)?;
            if inbox.workspace_id != workspace_id {
                return Err(RepositoryError::NotFound);
            }
            if inbox.status == InboxStatus::Revoked {
                return Ok(false);
            }
            validate_event(
                event,
                "inbox.revoked",
                &inbox.id,
                &inbox.workspace_id,
                revoked_at_ms,
            )?;
            let revoked_at = transfer_validation::to_i64(revoked_at_ms)?;
            if revoked_at_ms < inbox.created_at_ms {
                return Err(RepositoryError::InvalidInput);
            }
            let changed = transaction
                .execute(
                    "UPDATE inboxes SET status = 'revoked', revoked_at_ms = ?3 WHERE id = ?1 AND workspace_id = ?2 AND status = 'active'",
                    params![inbox_id, workspace_id, revoked_at],
                )
                .map_err(map_error)?;
            finish_audited_change(transaction, changed, event)
        })
    }
}

struct ValidatedInbox {
    expires_at_ms: i64,
    maximum_files: i64,
    maximum_bytes: i64,
    created_at_ms: i64,
}

fn validate_new_inbox(inbox: &NewInbox) -> Result<ValidatedInbox, RepositoryError> {
    for value in [&inbox.id, &inbox.workspace_id, &inbox.project_id] {
        rows::validate_text(value)?;
    }
    inbox_queries::validate_name(&inbox.name)?;
    inbox_queries::validate_capability(&inbox.capability_hash)?;
    let created_at_ms = transfer_validation::to_i64(inbox.created_at_ms)?;
    let expires_at_ms = transfer_validation::to_i64(inbox.expires_at_ms)?;
    if created_at_ms >= expires_at_ms || inbox.maximum_files == 0 || inbox.maximum_bytes == 0 {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(ValidatedInbox {
        expires_at_ms,
        maximum_files: transfer_validation::to_i64(inbox.maximum_files)?,
        maximum_bytes: transfer_validation::to_i64(inbox.maximum_bytes)?,
        created_at_ms,
    })
}

fn validate_event(
    event: &NewAuditEvent,
    action: &str,
    inbox_id: &str,
    workspace_id: &str,
    created_at_ms: u64,
) -> Result<(), RepositoryError> {
    let valid = event.action == action
        && event.target_type == "inbox"
        && event.workspace_id == workspace_id
        && event.created_at_ms == created_at_ms
        && event.metadata
            == [(
                "inboxId".to_owned(),
                blobyard_contract::AuditValue::String(inbox_id.to_owned()),
            )];
    if valid {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

#[cfg(test)]
#[path = "inboxes_tests.rs"]
mod tests;
