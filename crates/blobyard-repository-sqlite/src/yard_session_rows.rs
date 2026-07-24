use super::{rows, yard_rows};
use blobyard_contract::{
    NewYardContinuation, YardContinuationRecord, YardSessionListing, YardSessionRecord,
};
use rusqlite::Row;

pub(super) const CONTINUATION_COLUMNS: &str = "id, continuation_hash, code_hash, yard_id, environment_id, host_label, user_id, return_path, created_at_ms, expires_at_ms, consumed_at_ms";
pub(super) const SESSION_COLUMNS: &str = "id, token_hash, yard_id, environment_id, host_label, user_id, created_at_ms, expires_at_ms, last_used_at_ms, revoked_at_ms";

pub(super) fn continuation(row: &Row<'_>) -> rusqlite::Result<YardContinuationRecord> {
    Ok(YardContinuationRecord {
        continuation: NewYardContinuation {
            id: row.get(0)?,
            continuation_hash: row.get(1)?,
            code_hash: row.get(2)?,
            yard_id: row.get(3)?,
            environment_id: row.get(4)?,
            host_label: row.get(5)?,
            user_id: row.get(6)?,
            return_path: row.get(7)?,
            created_at_ms: yard_rows::required_u64(row.get(8)?)?,
            expires_at_ms: yard_rows::required_u64(row.get(9)?)?,
        },
        consumed_at_ms: yard_rows::optional_u64(row.get(10)?)?,
    })
}

pub(super) fn session(row: &Row<'_>) -> rusqlite::Result<YardSessionRecord> {
    Ok(YardSessionRecord {
        id: row.get(0)?,
        token_hash: row.get(1)?,
        yard_id: row.get(2)?,
        environment_id: row.get(3)?,
        host_label: row.get(4)?,
        user_id: row.get(5)?,
        created_at_ms: yard_rows::required_u64(row.get(6)?)?,
        expires_at_ms: yard_rows::required_u64(row.get(7)?)?,
        last_used_at_ms: yard_rows::optional_u64(row.get(8)?)?,
        revoked_at_ms: yard_rows::optional_u64(row.get(9)?)?,
    })
}

pub(super) fn listing(row: &Row<'_>) -> rusqlite::Result<YardSessionListing> {
    Ok(YardSessionListing {
        session: session(row)?,
        user_display_name: row.get(10)?,
    })
}

pub(super) fn validate_host_label(value: &str) -> Result<(), blobyard_contract::RepositoryError> {
    rows::validate_text(value)?;
    if value.contains('-') && blobyard_core::is_valid_dns_label(value) {
        Ok(())
    } else {
        Err(blobyard_contract::RepositoryError::InvalidInput)
    }
}

pub(super) fn validate_return_path(value: &str) -> Result<(), blobyard_contract::RepositoryError> {
    let valid = value.starts_with('/')
        && !value.starts_with("//")
        && !value.starts_with("/\\")
        && value.len() <= 2_048
        && !value.chars().any(char::is_control)
        && !value
            .strip_prefix('/')
            .is_some_and(|path| path == ".blobyard" || path.starts_with(".blobyard/"));
    if valid {
        Ok(())
    } else {
        Err(blobyard_contract::RepositoryError::InvalidInput)
    }
}

#[cfg(test)]
#[path = "yard_session_rows_tests.rs"]
mod tests;
