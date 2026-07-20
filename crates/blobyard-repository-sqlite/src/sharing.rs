use super::{
    SqliteRepository, auth_validation, finish_audited_change, lifecycle_audit, map_error, rows,
    sharing_queries, transfer_validation,
};
use blobyard_contract::{
    NewAuditEvent, NewDownloadGrant, NewShare, ObjectChecksum, RepositoryError, ShareRecord,
    ShareTarget, SharingRepository,
};
use rusqlite::params;

impl SharingRepository for SqliteRepository {
    fn create_share(
        &self,
        share: &NewShare,
        event: &NewAuditEvent,
    ) -> Result<ShareRecord, RepositoryError> {
        let times = validate_new_share(share)?;
        validate_event(
            event,
            "share.created",
            &share.id,
            &share.workspace_id,
            share.created_at_ms,
        )?;
        self.write_transaction(|transaction| {
            let changed = transaction
                .execute(
                    "INSERT INTO shares (id, workspace_id, version_id, capability_hash, expires_at_ms, status, consumed_count, maximum_downloads, created_at_ms, revoked_at_ms) SELECT ?1, ?2, v.id, ?4, ?5, 'active', 0, ?6, ?7, NULL FROM object_versions v JOIN projects p ON p.id = v.project_id WHERE v.id = ?3 AND v.state = 'complete' AND p.workspace_id = ?2",
                    params![
                        share.id,
                        share.workspace_id,
                        share.version_id,
                        share.capability_hash,
                        times.expires,
                        times.maximum_downloads,
                        times.created,
                    ],
                )
                .map_err(map_error)?;
            if changed != 1 {
                return Err(RepositoryError::NotFound);
            }
            lifecycle_audit::insert(transaction, event)?;
            sharing_queries::share_by_id(transaction, &share.id)
        })
    }

    fn list_shares(&self, workspace_id: &str) -> Result<Vec<ShareRecord>, RepositoryError> {
        rows::validate_text(workspace_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {} FROM shares WHERE workspace_id = ?1 ORDER BY created_at_ms DESC, id DESC",
                rows::SHARE_COLUMNS
            ))
            .map_err(map_error)?;
        let result = sharing_queries::list(&mut statement, workspace_id);
        drop(statement);
        drop(connection);
        result
    }

    fn share_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<ShareTarget, RepositoryError> {
        validate_capability(capability_hash)?;
        let connection = self.connection()?;
        sharing_queries::target_by_capability(
            &connection,
            capability_hash,
            transfer_validation::to_i64(now_ms)?,
            false,
        )
    }

    fn issue_share_download(
        &self,
        capability_hash: &str,
        now_ms: u64,
        grant: &NewDownloadGrant,
        event: &NewAuditEvent,
    ) -> Result<ShareTarget, RepositoryError> {
        validate_capability(capability_hash)?;
        let now = transfer_validation::to_i64(now_ms)?;
        validate_grant(grant, now_ms)?;
        self.write_transaction(|transaction| {
            let mut target =
                sharing_queries::target_by_capability(transaction, capability_hash, now, true)?;
            let grant_expires = transfer_validation::to_i64(grant.expires_at_ms)?;
            if grant.version_id != target.object.version.id
                || grant.expires_at_ms > target.share.expires_at_ms
            {
                return Err(RepositoryError::InvalidInput);
            }
            validate_event(
                event,
                "share.download_issued",
                &target.share.id,
                &target.share.workspace_id,
                now_ms,
            )?;
            transaction
                .execute(
                    "INSERT INTO download_grants (capability_hash, version_id, expires_at_ms) VALUES (?1, ?2, ?3)",
                    params![
                        grant.capability_hash,
                        grant.version_id,
                        grant_expires,
                    ],
                )
                .map_err(map_error)?;
            let changed = transaction
                .execute(
                    "UPDATE shares SET consumed_count = consumed_count + 1, status = CASE WHEN maximum_downloads = consumed_count + 1 THEN 'exhausted' ELSE 'active' END WHERE id = ?1 AND status = 'active' AND consumed_count < ?2",
                    params![target.share.id, i64::MAX],
                )
                .map_err(map_error)?;
            if changed != 1 {
                return Err(RepositoryError::InvalidInput);
            }
            lifecycle_audit::insert(transaction, event)?;
            target.share = sharing_queries::share_by_id(transaction, &target.share.id)?;
            Ok(target)
        })
    }

    fn revoke_share(
        &self,
        share_id: &str,
        workspace_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        rows::validate_text(share_id)?;
        rows::validate_text(workspace_id)?;
        self.write_transaction(|transaction| {
            let share = sharing_queries::share_by_id(transaction, share_id)?;
            if share.workspace_id != workspace_id {
                return Err(RepositoryError::NotFound);
            }
            if share.status == blobyard_contract::ShareStatus::Revoked {
                return Ok(false);
            }
            validate_event(
                event,
                "share.revoked",
                &share.id,
                &share.workspace_id,
                revoked_at_ms,
            )?;
            let changed = transaction
                .execute(
                    "UPDATE shares SET status = 'revoked', revoked_at_ms = ?3 WHERE id = ?1 AND workspace_id = ?2 AND status != 'revoked'",
                    params![
                        share_id,
                        workspace_id,
                        transfer_validation::to_i64(revoked_at_ms)?,
                    ],
                )
                .map_err(map_error)?;
            finish_audited_change(transaction, changed, event)
        })
    }
}

struct ShareTimes {
    expires: i64,
    maximum_downloads: Option<i64>,
    created: i64,
}

fn validate_new_share(share: &NewShare) -> Result<ShareTimes, RepositoryError> {
    for value in [&share.id, &share.workspace_id, &share.version_id] {
        rows::validate_text(value)?;
    }
    validate_capability(&share.capability_hash)?;
    let expires = transfer_validation::to_i64(share.expires_at_ms)?;
    let maximum_downloads = share
        .maximum_downloads
        .map(transfer_validation::to_i64)
        .transpose()?;
    let created = transfer_validation::to_i64(share.created_at_ms)?;
    if share.created_at_ms >= share.expires_at_ms || share.maximum_downloads == Some(0) {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(ShareTimes {
        expires,
        maximum_downloads,
        created,
    })
}

fn validate_capability(value: &str) -> Result<(), RepositoryError> {
    auth_validation::validate_hash(value)
}

fn validate_grant(grant: &NewDownloadGrant, now_ms: u64) -> Result<(), RepositoryError> {
    rows::validate_text(&grant.version_id)?;
    ObjectChecksum::new(&grant.capability_hash).map_err(|_error| RepositoryError::InvalidInput)?;
    if grant.expires_at_ms <= now_ms {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_event(
    event: &NewAuditEvent,
    action: &str,
    share_id: &str,
    workspace_id: &str,
    created_at_ms: u64,
) -> Result<(), RepositoryError> {
    let is_valid = event.action == action
        && event.target_type == "share"
        && event.workspace_id == workspace_id
        && event.created_at_ms == created_at_ms
        && event.metadata
            == [(
                "shareId".to_owned(),
                blobyard_contract::AuditValue::String(share_id.to_owned()),
            )];
    if is_valid {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

#[cfg(test)]
#[path = "sharing_tests.rs"]
mod tests;
