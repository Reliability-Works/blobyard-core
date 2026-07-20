use super::{ci_match, ci_records, map_error, rows};
use blobyard_contract::{LocalMachineSessionRecord, RepositoryError};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) fn validate_and_touch(
    connection: &Connection,
    token_id: &str,
    now_ms: u64,
    now: i64,
) -> Result<(), RepositoryError> {
    let Some(session) = machine_session(connection, token_id)? else {
        return Ok(());
    };
    let trust = ci_records::trust_by_id(connection, &session.trust_id, &session.workspace_id)?;
    if !ci_match::session_trust_valid(&session, &trust, now_ms) {
        return Err(RepositoryError::NotFound);
    }
    connection
        .execute(
            "UPDATE machine_sessions SET last_used_at_ms = CASE WHEN last_used_at_ms IS NULL OR last_used_at_ms < ?2 THEN ?2 ELSE last_used_at_ms END WHERE token_id = ?1",
            params![token_id, now],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

fn machine_session(
    connection: &Connection,
    token_id: &str,
) -> Result<Option<LocalMachineSessionRecord>, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM machine_sessions WHERE token_id = ?1",
                rows::MACHINE_SESSION_COLUMNS
            ),
            [token_id],
            rows::machine_session,
        )
        .optional()
        .map_err(map_error)
}
