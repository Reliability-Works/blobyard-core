use super::{rows, validate_record};
use blobyard_contract::{
    AuditValue, LocalApiTokenRecord, LocalCliSessionRecord, NewAuditEvent, ObjectChecksum,
    RepositoryError,
};

pub(super) fn validate_hash(value: &str) -> Result<(), RepositoryError> {
    match ObjectChecksum::new(value) {
        Ok(_checksum) => Ok(()),
        Err(_error) => Err(RepositoryError::InvalidInput),
    }
}

pub(super) fn validate_token(token: &LocalApiTokenRecord) -> Result<(), RepositoryError> {
    validate_record(&token.id, &token.name)?;
    validate_hash(&token.secret_hash)?;
    rows::validate_text(&token.token_prefix)?;
    rows::validate_text(&token.workspace_id)?;
    if let Some(project_id) = token.project_id.as_deref() {
        rows::validate_text(project_id)?;
    }
    if token.scopes.is_empty()
        || token.created_at_ms >= token.expires_at_ms
        || token
            .last_used_at_ms
            .is_some_and(|value| value < token.created_at_ms)
        || token
            .revoked_at_ms
            .is_some_and(|value| value < token.created_at_ms)
        || token
            .scopes
            .iter()
            .any(|scope| rows::validate_text(scope).is_err())
    {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(())
}

pub(super) fn validate_session(
    session: &LocalCliSessionRecord,
    token: &LocalApiTokenRecord,
) -> Result<(), RepositoryError> {
    for value in [
        &session.id,
        &session.token_id,
        &session.workspace_id,
        &session.name,
        &session.platform,
        &session.version,
    ] {
        rows::validate_text(value)?;
    }
    let identity_matches = (
        session.token_id.as_str(),
        session.workspace_id.as_str(),
        session.name.as_str(),
        session.created_at_ms,
    ) == (
        token.id.as_str(),
        token.workspace_id.as_str(),
        token.name.as_str(),
        token.created_at_ms,
    );
    if !identity_matches || session.last_used_at_ms.is_some() || session.revoked_at_ms.is_some() {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(())
}

pub(super) fn validate_session_event(
    event: &NewAuditEvent,
    session_id: &str,
    workspace_id: &str,
    created_at_ms: u64,
) -> Result<(), RepositoryError> {
    if event.action != "cli.session_revoked"
        || event.target_type != "cli_session"
        || event.workspace_id != workspace_id
        || event.created_at_ms != created_at_ms
        || event.metadata
            != [(
                "sessionId".to_owned(),
                AuditValue::String(session_id.to_owned()),
            )]
    {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(())
}

pub(super) fn validate_token_event(
    event: &NewAuditEvent,
    action: &str,
    token_id: &str,
    workspace_id: &str,
    created_at_ms: u64,
) -> Result<(), RepositoryError> {
    if event.action != action
        || event.target_type != "api_token"
        || event.workspace_id != workspace_id
        || event.created_at_ms != created_at_ms
        || event.metadata
            != [(
                "tokenId".to_owned(),
                AuditValue::String(token_id.to_owned()),
            )]
    {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(())
}

pub(super) fn sql_time(value: u64) -> Result<i64, RepositoryError> {
    i64::try_from(value).map_err(|_error| RepositoryError::InvalidInput)
}

pub(super) fn optional_sql_time(value: Option<u64>) -> Result<Option<i64>, RepositoryError> {
    value.map(sql_time).transpose()
}
