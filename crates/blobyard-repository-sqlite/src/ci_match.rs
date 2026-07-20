use super::{ci_validation, map_error, rows};
use blobyard_contract::{
    LocalCiTrustRecord, LocalMachineSessionRecord, NewMachineSession, RepositoryError,
};
use rusqlite::{Connection, OptionalExtension, ToSql, params};

pub(super) fn matching_trust(
    connection: &Connection,
    session: &NewMachineSession,
) -> Result<Option<(LocalCiTrustRecord, String)>, RepositoryError> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {} FROM ci_trusts WHERE repository = ?1 AND revoked_at_ms IS NULL ORDER BY project_id IS NOT NULL DESC, created_at_ms DESC, id DESC",
            rows::CI_TRUST_COLUMNS
        ))
        .map_err(map_error)?;
    let parameters: [&dyn ToSql; 1] = [&session.identity.repository];
    let trusts = super::ci_records::query_trusts(&mut statement, &parameters)?;
    drop(statement);
    for trust in trusts {
        if claims_match(&trust, session)
            && let Some(project_id) = scoped_project(connection, &trust, session)?
        {
            return Ok(Some((trust, project_id)));
        }
    }
    Ok(None)
}

pub(super) fn assertion_exists(
    connection: &Connection,
    assertion_hash: &str,
) -> Result<bool, RepositoryError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM machine_sessions WHERE oidc_token_hash = ?1)",
            [assertion_hash],
            |row| row.get(0),
        )
        .map_err(map_error)
}

pub(super) fn rate_retry(
    connection: &Connection,
    trust_id: &str,
    now_ms: u64,
    now: i64,
) -> Result<Option<u64>, RepositoryError> {
    let cutoff = now.saturating_sub(ci_validation::EXCHANGE_RATE_WINDOW_MS.cast_signed());
    let oldest = connection
        .query_row(
            "SELECT MIN(created_at_ms) FROM machine_sessions WHERE trust_id = ?1 AND created_at_ms > ?2 HAVING COUNT(*) >= ?3",
            params![
                trust_id,
                cutoff,
                i64::from(ci_validation::EXCHANGE_RATE_LIMIT),
            ],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()
        .map_err(map_error)?
        .flatten();
    oldest
        .map(|value| {
            let created = value.cast_unsigned();
            let ready_at = created.saturating_add(ci_validation::EXCHANGE_RATE_WINDOW_MS);
            Ok(ready_at.saturating_sub(now_ms).div_ceil(1_000).max(1))
        })
        .transpose()
}

pub(super) fn session_trust_valid(
    session: &LocalMachineSessionRecord,
    trust: &LocalCiTrustRecord,
    now_ms: u64,
) -> bool {
    session.revoked_at_ms.is_none()
        && session.expires_at_ms > now_ms
        && trust.revoked_at_ms.is_none()
        && trust.repository == session.repository
        && trust
            .project_id
            .as_ref()
            .is_none_or(|id| id == &session.project_id)
        && session
            .actions
            .iter()
            .all(|action| trust.allowed_actions.contains(action))
}

fn claims_match(trust: &LocalCiTrustRecord, session: &NewMachineSession) -> bool {
    trust.audience == session.identity.audience
        && trust.workflow_path == session.identity.workflow_path
        && trust.workflow_ref == session.identity.workflow_ref
        && trust.environment == session.identity.environment
        && ci_validation::ref_matches(&session.identity.git_ref, &trust.allowed_ref_glob)
        && session
            .actions
            .iter()
            .all(|action| trust.allowed_actions.contains(action))
}

fn scoped_project(
    connection: &Connection,
    trust: &LocalCiTrustRecord,
    session: &NewMachineSession,
) -> Result<Option<String>, RepositoryError> {
    let workspace_slug = connection
        .query_row(
            "SELECT slug FROM workspaces WHERE id = ?1",
            [&trust.workspace_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(map_error)?;
    if session
        .workspace
        .as_ref()
        .is_some_and(|requested| requested != &workspace_slug)
    {
        return Ok(None);
    }
    connection
        .query_row(
            "SELECT id FROM projects WHERE workspace_id = ?1 AND slug = ?2 AND (?3 IS NULL OR id = ?3)",
            params![trust.workspace_id, session.project, trust.project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_error)
}
