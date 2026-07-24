use super::{map_error, rows, yard_session_rows};
use blobyard_contract::{RepositoryError, YardAdmission};
use rusqlite::{Connection, OptionalExtension, Row, params};

pub(super) fn evaluate(
    connection: &Connection,
    host_label: &str,
    user_id: &str,
    now_ms: i64,
) -> Result<YardAdmission, RepositoryError> {
    yard_session_rows::validate_host_label(host_label)?;
    rows::validate_text(user_id)?;
    connection
        .query_row(
            "SELECT y.id, e.id, y.workspace_id
             FROM web_yards y
             JOIN yard_environments e
               ON e.yard_id = y.id AND e.kind = 'production' AND e.status = 'active'
             JOIN local_users u ON u.id = ?2 AND u.status = 'active'
             LEFT JOIN yard_access_policies p ON p.yard_id = y.id
             WHERE y.status = 'active'
               AND (
                 y.host_label = ?1
                 OR EXISTS (
                   SELECT 1 FROM yard_deploys d
                   WHERE d.yard_id = y.id
                     AND d.deployment_host_label = ?1
                     AND d.status IN ('live', 'superseded')
                 )
               )
               AND (
                 p.yard_id IS NULL
                 OR p.visibility = 'public'
                 OR p.visibility = 'any-authenticated'
                 OR (p.visibility = 'workspace' AND u.workspace_id = y.workspace_id)
                 OR (
                   p.visibility IN ('selected', 'authenticated-link')
                   AND EXISTS (
                     SELECT 1 FROM yard_access_grants g
                     WHERE g.yard_id = y.id
                       AND g.principal_kind = 'user'
                       AND g.principal_id = u.id
                       AND g.status = 'active'
                       AND (g.expires_at_ms IS NULL OR g.expires_at_ms > ?3)
                       AND (g.environment_id IS NULL OR g.environment_id = e.id)
                   )
                 )
               )
             LIMIT 1",
            params![host_label, user_id, now_ms],
            admission,
        )
        .map_err(map_error)
}

fn admission(row: &Row<'_>) -> rusqlite::Result<YardAdmission> {
    Ok(YardAdmission {
        yard_id: row.get(0)?,
        environment_id: row.get(1)?,
        workspace_id: row.get(2)?,
    })
}

pub(super) fn session_id(
    connection: &Connection,
    token_hash: &str,
    host_label: &str,
    yard_id: &str,
    visibility: &str,
    now_ms: i64,
) -> Result<Option<String>, RepositoryError> {
    connection
        .query_row(
            "SELECT s.id
             FROM yard_sessions s
             JOIN local_users u ON u.id = s.user_id AND u.status = 'active'
             JOIN web_yards y ON y.id = s.yard_id AND y.status = 'active'
             JOIN yard_environments e
               ON e.id = s.environment_id
              AND e.yard_id = s.yard_id
              AND e.kind = 'production'
              AND e.status = 'active'
             WHERE s.token_hash = ?1
               AND s.host_label = ?2
               AND s.yard_id = ?3
               AND s.revoked_at_ms IS NULL
               AND s.expires_at_ms > ?5
               AND (
                 ?4 = 'any-authenticated'
                 OR (?4 = 'workspace' AND u.workspace_id = y.workspace_id)
                 OR (
                   ?4 IN ('selected', 'authenticated-link')
                   AND EXISTS (
                     SELECT 1 FROM yard_access_grants g
                     WHERE g.yard_id = s.yard_id
                       AND g.status = 'active'
                       AND g.principal_id = s.user_id
                       AND g.principal_kind = 'user'
                       AND (g.environment_id IS NULL OR g.environment_id = s.environment_id)
                       AND (g.expires_at_ms IS NULL OR g.expires_at_ms > ?5)
                   )
                 )
               )",
            params![token_hash, host_label, yard_id, visibility, now_ms],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_error)
}

#[cfg(test)]
#[path = "yard_session_admission_tests.rs"]
mod tests;
