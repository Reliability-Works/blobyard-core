use super::{map_error, rows, yard_session_rows};
use blobyard_contract::{RepositoryError, YardAdmission};
use rusqlite::{Connection, OptionalExtension, Row, params};

macro_rules! selected_grant_admission_sql {
    ($now:literal) => {
        concat!(
            "(
              EXISTS (
                SELECT 1
                FROM yard_access_grants direct_grant
                WHERE direct_grant.yard_id = y.id
                  AND direct_grant.principal_kind = 'user'
                  AND direct_grant.principal_id = u.id
                  AND u.workspace_id = y.workspace_id
                  AND direct_grant.status = 'active'
                  AND direct_grant.revoked_at_ms IS NULL
                  AND (direct_grant.expires_at_ms IS NULL OR direct_grant.expires_at_ms > ",
            $now,
            ")
                  AND (
                    direct_grant.environment_id IS NULL
                    OR direct_grant.environment_id = e.id
                  )
                LIMIT 1
              )
              OR (
                u.workspace_id = y.workspace_id
                AND (
                  SELECT COUNT(*)
                  FROM (
                    SELECT 1
                    FROM workspace_group_members bounded_user_memberships
                    WHERE bounded_user_memberships.user_id = u.id
                      AND bounded_user_memberships.workspace_id = y.workspace_id
                    LIMIT 101
                  )
                ) <= 100
                AND EXISTS (
                  SELECT 1
                  FROM (
                    SELECT group_id, workspace_id, user_id, added_at_ms
                    FROM workspace_group_members selected_memberships
                    WHERE selected_memberships.user_id = u.id
                      AND selected_memberships.workspace_id = y.workspace_id
                    LIMIT 101
                  ) gm
                  JOIN workspace_groups wg
                    ON wg.id = gm.group_id
                   AND wg.workspace_id = gm.workspace_id
                  WHERE wg.status = 'active'
                    AND wg.deactivated_at_ms IS NULL
                    AND wg.created_at_ms >= 0
                    AND wg.member_count BETWEEN 0 AND 500
                    AND gm.added_at_ms >= 0
                    AND wg.member_count = (
                      SELECT COUNT(*)
                      FROM (
                        SELECT 1
                        FROM workspace_group_members counted
                        WHERE counted.group_id = wg.id
                          AND counted.workspace_id = wg.workspace_id
                        LIMIT 501
                      )
                    )
                    AND NOT EXISTS (
                      SELECT 1
                      FROM (
                        SELECT user_id, workspace_id, added_at_ms
                        FROM workspace_group_members bounded_members
                        WHERE bounded_members.group_id = wg.id
                          AND bounded_members.workspace_id = wg.workspace_id
                        LIMIT 501
                      ) checked
                      LEFT JOIN local_users checked_user
                        ON checked_user.id = checked.user_id
                       AND checked_user.workspace_id = checked.workspace_id
                      WHERE checked.added_at_ms < 0
                         OR checked_user.id IS NULL
                         OR checked_user.status != 'active'
                         OR checked_user.deactivated_at_ms IS NOT NULL
                    )
                    AND EXISTS (
                      SELECT 1
                      FROM yard_access_grants group_grant
                      WHERE group_grant.yard_id = y.id
                        AND group_grant.principal_kind = 'group'
                        AND group_grant.principal_id = wg.id
                        AND group_grant.status = 'active'
                        AND group_grant.revoked_at_ms IS NULL
                        AND (
                          group_grant.expires_at_ms IS NULL
                          OR group_grant.expires_at_ms > ",
            $now,
            "
                        )
                        AND (
                          group_grant.environment_id IS NULL
                          OR group_grant.environment_id = e.id
                        )
                        AND (
                          SELECT COUNT(*)
                          FROM (
                            SELECT 1
                            FROM yard_access_grants bounded_group_grants
                            WHERE bounded_group_grants.principal_kind = 'group'
                              AND bounded_group_grants.principal_id = wg.id
                              AND bounded_group_grants.status = 'active'
                            LIMIT 501
                          )
                        ) <= 500
                      LIMIT 1
                    )
                  LIMIT 1
                )
              )
            )"
        )
    };
}

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
            concat!(
                "SELECT y.id, e.id, y.workspace_id
             FROM web_yards y
             JOIN yard_environments e
               ON e.yard_id = y.id AND e.kind = 'production' AND e.status = 'active'
             JOIN local_users u
               ON u.id = ?2
              AND u.status = 'active'
              AND u.deactivated_at_ms IS NULL
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
                   AND ",
                selected_grant_admission_sql!("?3"),
                "
                 )
               )
             LIMIT 1"
            ),
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
            concat!(
                "SELECT s.id
             FROM yard_sessions s
             JOIN local_users u
               ON u.id = s.user_id
              AND u.status = 'active'
              AND u.deactivated_at_ms IS NULL
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
                   AND ",
                selected_grant_admission_sql!("?5"),
                "
                 )
               )"
            ),
            params![token_hash, host_label, yard_id, visibility, now_ms],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_error)
}

#[cfg(test)]
#[path = "yard_session_admission_tests.rs"]
mod tests;
