use super::{
    SqliteRepository, auth, auth_validation, ci_match, ci_records, ci_validation, lifecycle_audit,
    map_error, rows,
};
use blobyard_contract::{
    CiRepository, LocalCiTrustRecord, LocalMachineSessionRecord, MachineSessionMintResult,
    NewAuditEvent, NewMachineSession, RepositoryError,
};
use rusqlite::{ToSql, Transaction, params};

impl CiRepository for SqliteRepository {
    fn create_ci_trust(
        &self,
        trust: &LocalCiTrustRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        let created_at = ci_validation::validate_trust(trust)?;
        ci_validation::validate_event(
            event,
            "ci.trust_created",
            "ci_trust",
            &trust.id,
            &trust.repository,
            &trust.workspace_id,
            trust.created_at_ms,
        )?;
        self.write_transaction(|transaction| {
            ci_records::require_project_scope(
                transaction,
                trust.project_id.as_deref(),
                &trust.workspace_id,
            )?;
            ci_records::insert_trust(transaction, trust, created_at)?;
            lifecycle_audit::insert(transaction, event)
        })
    }

    fn list_ci_trusts(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<LocalCiTrustRecord>, RepositoryError> {
        rows::validate_text(workspace_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {} FROM ci_trusts WHERE workspace_id = ?1 ORDER BY created_at_ms DESC, id DESC",
                rows::CI_TRUST_COLUMNS
            ))
            .map_err(map_error)?;
        let parameters: [&dyn ToSql; 1] = [&workspace_id];
        let result = ci_records::query_trusts(&mut statement, &parameters);
        drop(statement);
        drop(connection);
        result
    }

    fn revoke_ci_trust(
        &self,
        id: &str,
        workspace_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        rows::validate_text(id)?;
        rows::validate_text(workspace_id)?;
        let now = auth_validation::sql_time(now_ms)?;
        self.write_transaction(|transaction| {
            let trust = ci_records::trust_by_id(transaction, id, workspace_id)?;
            if trust.revoked_at_ms.is_some() {
                return Ok(false);
            }
            ci_validation::validate_event(
                event,
                "ci.trust_revoked",
                "ci_trust",
                id,
                &trust.repository,
                workspace_id,
                now_ms,
            )?;
            transaction
                .execute(
                    "UPDATE ci_trusts SET revoked_at_ms = ?2 WHERE id = ?1 AND revoked_at_ms IS NULL",
                    params![id, now],
                )
                .map_err(map_error)?;
            transaction
                .execute(
                    "UPDATE api_tokens SET revoked = 1, revoked_at_ms = ?2 WHERE id IN (SELECT token_id FROM machine_sessions WHERE trust_id = ?1) AND revoked = 0",
                    params![id, now],
                )
                .map_err(map_error)?;
            transaction
                .execute(
                    "UPDATE machine_sessions SET revoked_at_ms = ?2 WHERE trust_id = ?1 AND revoked_at_ms IS NULL",
                    params![id, now],
                )
                .map_err(map_error)?;
            lifecycle_audit::insert(transaction, event)?;
            Ok(true)
        })
    }

    fn mint_machine_session(
        &self,
        session: &NewMachineSession,
        event: &NewAuditEvent,
    ) -> Result<MachineSessionMintResult, RepositoryError> {
        let times = ci_validation::validate_exchange(session)?;
        self.write_transaction(|transaction| mint(transaction, session, event, times))
    }

    fn authenticate_machine_session(
        &self,
        token_id: &str,
        now_ms: u64,
    ) -> Result<LocalMachineSessionRecord, RepositoryError> {
        rows::validate_text(token_id)?;
        let now = auth_validation::sql_time(now_ms)?;
        self.write_transaction(|transaction| {
            let session = transaction
                .query_row(
                    &format!(
                        "SELECT {} FROM machine_sessions WHERE token_id = ?1 AND revoked_at_ms IS NULL AND expires_at_ms > ?2",
                        rows::MACHINE_SESSION_COLUMNS
                    ),
                    params![token_id, now],
                    rows::machine_session,
                )
                .map_err(map_error)?;
            let trust = ci_records::trust_by_id(
                transaction,
                &session.trust_id,
                &session.workspace_id,
            )?;
            if !ci_match::session_trust_valid(&session, &trust, now_ms) {
                return Err(RepositoryError::NotFound);
            }
            transaction
                .execute(
                    "UPDATE machine_sessions SET last_used_at_ms = CASE WHEN last_used_at_ms IS NULL OR last_used_at_ms < ?2 THEN ?2 ELSE last_used_at_ms END WHERE token_id = ?1",
                    params![token_id, now],
                )
                .map_err(map_error)?;
            Ok(LocalMachineSessionRecord {
                last_used_at_ms: Some(now_ms.max(session.last_used_at_ms.unwrap_or(0))),
                ..session
            })
        })
    }
}

fn mint(
    transaction: &Transaction<'_>,
    session: &NewMachineSession,
    event: &NewAuditEvent,
    times: ci_validation::SqlMachineTimes,
) -> Result<MachineSessionMintResult, RepositoryError> {
    if ci_match::assertion_exists(transaction, &session.oidc_token_hash)? {
        return Ok(MachineSessionMintResult::Replayed);
    }
    let Some((trust, project_id)) = ci_match::matching_trust(transaction, session)? else {
        return Ok(MachineSessionMintResult::Forbidden);
    };
    if let Some(retry_after_seconds) =
        ci_match::rate_retry(transaction, &trust.id, session.now_ms, times.now)?
    {
        return Ok(MachineSessionMintResult::RateLimited {
            retry_after_seconds,
        });
    }
    ci_validation::validate_event(
        event,
        "ci.token_minted",
        "project",
        &project_id,
        &trust.repository,
        &trust.workspace_id,
        session.now_ms,
    )?;
    let record = ci_records::machine_record(session, &trust, project_id);
    let token = ci_records::machine_token(session, &record);
    auth::insert_token(transaction, &token)?;
    ci_records::insert_machine(transaction, session, &record, times)?;
    lifecycle_audit::insert(transaction, event)?;
    Ok(MachineSessionMintResult::Minted(Box::new(record)))
}

#[cfg(test)]
#[path = "ci_tests.rs"]
mod tests;
