use super::{auth_validation, rows, yard_session_rows};
use blobyard_contract::{
    NewYardContinuation, NewYardSession, RepositoryError, YARD_EXCHANGE_CODE_LIFETIME_MS,
    YARD_SESSION_LIFETIME_MS, YardContinuationRecord, YardSessionAuditContext, YardSessionRecord,
};

pub(super) fn continuation(value: &NewYardContinuation) -> Result<(), RepositoryError> {
    for text in [
        &value.id,
        &value.yard_id,
        &value.environment_id,
        &value.user_id,
    ] {
        rows::validate_text(text)?;
    }
    auth_validation::validate_hash(&value.continuation_hash)?;
    auth_validation::validate_hash(&value.code_hash)?;
    yard_session_rows::validate_host_label(&value.host_label)?;
    yard_session_rows::validate_return_path(&value.return_path)?;
    let expected = value
        .created_at_ms
        .checked_add(YARD_EXCHANGE_CODE_LIFETIME_MS);
    if expected == Some(value.expires_at_ms) {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

pub(super) fn session(
    session: &NewYardSession,
    audit: &YardSessionAuditContext,
    now_ms: u64,
) -> Result<(), RepositoryError> {
    rows::validate_text(&session.id)?;
    rows::validate_text(&audit.id)?;
    rows::validate_text(&audit.request_id)?;
    auth_validation::validate_hash(&session.token_hash)?;
    let expected = session.created_at_ms.checked_add(YARD_SESSION_LIFETIME_MS);
    if session.created_at_ms == now_ms && expected == Some(session.expires_at_ms) {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

pub(super) fn record(
    session: &NewYardSession,
    continuation: &YardContinuationRecord,
) -> YardSessionRecord {
    YardSessionRecord {
        id: session.id.clone(),
        token_hash: session.token_hash.clone(),
        yard_id: continuation.continuation.yard_id.clone(),
        environment_id: continuation.continuation.environment_id.clone(),
        host_label: continuation.continuation.host_label.clone(),
        user_id: continuation.continuation.user_id.clone(),
        created_at_ms: session.created_at_ms,
        expires_at_ms: session.expires_at_ms,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}
