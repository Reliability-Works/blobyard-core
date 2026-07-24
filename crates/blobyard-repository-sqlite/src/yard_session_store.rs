use super::{
    lifecycle_audit, map_error, yard_session_admission, yard_session_rows, yard_session_validation,
    yard_validation,
};
use blobyard_contract::{
    AuditValue, NewAuditEvent, NewYardContinuation, NewYardSession, RepositoryError,
    YardContinuationRecord, YardSessionAuditContext, YardSessionExchange, YardSessionListing,
    YardSessionRecord,
};
use rusqlite::{Connection, OptionalExtension, Statement, Transaction, params};

const CONTINUATION_HISTORY_MS: u64 = 86_400_000;
const SESSION_HISTORY_MS: u64 = 2_592_000_000;

pub(super) fn issue(
    transaction: &Transaction<'_>,
    continuation: &NewYardContinuation,
) -> Result<(), RepositoryError> {
    yard_session_validation::continuation(continuation)?;
    let now = super::auth_validation::sql_time(continuation.created_at_ms)?;
    let admission = yard_session_admission::evaluate(
        transaction,
        &continuation.host_label,
        &continuation.user_id,
        now,
    )?;
    if admission.yard_id != continuation.yard_id
        || admission.environment_id != continuation.environment_id
    {
        return Err(RepositoryError::NotFound);
    }
    transaction
        .execute(
            "INSERT INTO yard_continuations (id, continuation_hash, code_hash, yard_id, environment_id, host_label, user_id, return_path, created_at_ms, expires_at_ms, consumed_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)",
            params![
                continuation.id,
                continuation.continuation_hash,
                continuation.code_hash,
                continuation.yard_id,
                continuation.environment_id,
                continuation.host_label,
                continuation.user_id,
                continuation.return_path,
                now,
                super::auth_validation::sql_time(continuation.expires_at_ms)?,
            ],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

pub(super) fn exchange(
    transaction: &Transaction<'_>,
    code_hash: &str,
    host_label: &str,
    session: &NewYardSession,
    audit: &YardSessionAuditContext,
    now_ms: u64,
) -> Result<YardSessionExchange, RepositoryError> {
    super::auth_validation::validate_hash(code_hash)?;
    yard_session_rows::validate_host_label(host_label)?;
    yard_session_validation::session(session, audit, now_ms)?;
    let now = super::auth_validation::sql_time(now_ms)?;
    let continuation = consume_continuation(transaction, code_hash, host_label, now)?;
    let admission = yard_session_admission::evaluate(
        transaction,
        host_label,
        &continuation.continuation.user_id,
        now,
    )?;
    if admission.yard_id != continuation.continuation.yard_id
        || admission.environment_id != continuation.continuation.environment_id
    {
        return Err(RepositoryError::NotFound);
    }
    let record = yard_session_validation::record(session, &continuation);
    insert_session(transaction, &record)?;
    insert_issued_audit(transaction, audit, &admission, &record, now_ms)?;
    Ok(YardSessionExchange {
        session: record,
        return_path: continuation.continuation.return_path,
    })
}

fn consume_continuation(
    transaction: &Transaction<'_>,
    code_hash: &str,
    host_label: &str,
    now: i64,
) -> Result<YardContinuationRecord, RepositoryError> {
    transaction
        .query_row(
            &format!(
                "UPDATE yard_continuations SET consumed_at_ms = ?3 WHERE code_hash = ?1 AND host_label = ?2 AND consumed_at_ms IS NULL AND expires_at_ms > ?3 RETURNING {}",
                yard_session_rows::CONTINUATION_COLUMNS
            ),
            params![code_hash, host_label, now],
            yard_session_rows::continuation,
        )
        .map_err(map_error)
}

fn insert_session(
    transaction: &Transaction<'_>,
    session: &YardSessionRecord,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "INSERT INTO yard_sessions (id, token_hash, yard_id, environment_id, host_label, user_id, created_at_ms, expires_at_ms, last_used_at_ms, revoked_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL)",
            params![
                session.id,
                session.token_hash,
                session.yard_id,
                session.environment_id,
                session.host_label,
                session.user_id,
                super::auth_validation::sql_time(session.created_at_ms)?,
                super::auth_validation::sql_time(session.expires_at_ms)?,
            ],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

fn insert_issued_audit(
    transaction: &Transaction<'_>,
    audit: &YardSessionAuditContext,
    admission: &blobyard_contract::YardAdmission,
    session: &YardSessionRecord,
    now_ms: u64,
) -> Result<(), RepositoryError> {
    lifecycle_audit::insert(
        transaction,
        &NewAuditEvent {
            id: audit.id.clone(),
            workspace_id: admission.workspace_id.clone(),
            actor: session.user_id.clone(),
            action: "yard.session_issued".to_owned(),
            request_id: audit.request_id.clone(),
            target_type: "yard_session".to_owned(),
            metadata: vec![
                (
                    "sessionId".to_owned(),
                    AuditValue::String(session.id.clone()),
                ),
                (
                    "yardId".to_owned(),
                    AuditValue::String(session.yard_id.clone()),
                ),
            ],
            created_at_ms: now_ms,
        },
    )
}

pub(super) fn list(
    statement: &mut Statement<'_>,
    yard_id: &str,
) -> Result<Vec<YardSessionListing>, RepositoryError> {
    super::collect(
        statement
            .query_map([yard_id], yard_session_rows::listing)
            .map_err(map_error)?,
    )
}

pub(super) fn revoke(
    transaction: &Transaction<'_>,
    yard_id: &str,
    session_id: &str,
    now_ms: u64,
    event: &NewAuditEvent,
) -> Result<bool, RepositoryError> {
    let session = session_by_id(transaction, session_id)?.ok_or(RepositoryError::NotFound)?;
    if session.yard_id != yard_id {
        return Err(RepositoryError::NotFound);
    }
    if session.revoked_at_ms.is_some() {
        return Ok(false);
    }
    let yard = super::yard_queries::yard_by_id(transaction, yard_id)?;
    let revoked_at = yard_validation::action_event(
        event,
        "yard.session_revoked",
        "yard_session",
        &yard.workspace_id,
        now_ms,
        [
            ("sessionId", AuditValue::String(session.id.clone())),
            ("yardId", AuditValue::String(yard.id)),
        ],
    )?;
    let changed = transaction
        .execute(
            "UPDATE yard_sessions SET revoked_at_ms = ?2 WHERE id = ?1 AND revoked_at_ms IS NULL",
            params![session.id, revoked_at],
        )
        .map_err(map_error)?;
    super::changed_once(changed)?;
    lifecycle_audit::insert(transaction, event)?;
    Ok(true)
}

pub(super) fn revoke_by_token(
    connection: &Connection,
    token_hash: &str,
    host_label: &str,
    now_ms: u64,
) -> Result<bool, RepositoryError> {
    super::auth_validation::validate_hash(token_hash)?;
    yard_session_rows::validate_host_label(host_label)?;
    let now = super::auth_validation::sql_time(now_ms)?;
    connection
        .execute(
            "UPDATE yard_sessions SET revoked_at_ms = ?3 WHERE token_hash = ?1 AND host_label = ?2 AND revoked_at_ms IS NULL AND expires_at_ms > ?3",
            params![token_hash, host_label, now],
        )
        .map(|changed| changed == 1)
        .map_err(map_error)
}

pub(super) fn revoke_for_user(
    transaction: &Transaction<'_>,
    user_id: &str,
    now_ms: i64,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "UPDATE yard_sessions SET revoked_at_ms = ?2 WHERE user_id = ?1 AND revoked_at_ms IS NULL AND expires_at_ms > ?2",
            params![user_id, now_ms],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

pub(super) fn purge(transaction: &Transaction<'_>, now_ms: u64) -> Result<(), RepositoryError> {
    let continuation_before =
        super::auth_validation::sql_time(now_ms.saturating_sub(CONTINUATION_HISTORY_MS))?;
    let session_before =
        super::auth_validation::sql_time(now_ms.saturating_sub(SESSION_HISTORY_MS))?;
    transaction
        .execute(
            "DELETE FROM yard_continuations WHERE expires_at_ms < ?1",
            [continuation_before],
        )
        .map_err(map_error)?;
    transaction
        .execute(
            "DELETE FROM yard_sessions WHERE expires_at_ms < ?1 OR (revoked_at_ms IS NOT NULL AND revoked_at_ms < ?1)",
            [session_before],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

fn session_by_id(
    connection: &Connection,
    session_id: &str,
) -> Result<Option<YardSessionRecord>, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM yard_sessions WHERE id = ?1",
                yard_session_rows::SESSION_COLUMNS
            ),
            [session_id],
            yard_session_rows::session,
        )
        .optional()
        .map_err(map_error)
}
