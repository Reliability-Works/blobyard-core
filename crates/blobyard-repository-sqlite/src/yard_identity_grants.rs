use super::{map_error, rows, yard_access};
use blobyard_contract::{MAXIMUM_USER_GROUPS, RepositoryError};
use rusqlite::{Connection, params};

const GROUP_ROLES_SQL: &str = "
    SELECT wg.id, grant.app_roles
    FROM workspace_group_members membership
    JOIN workspace_groups wg
      ON wg.id = membership.group_id
     AND wg.workspace_id = membership.workspace_id
    JOIN yard_access_grants grant
      ON grant.principal_kind = 'group'
     AND grant.principal_id = wg.id
     AND grant.yard_id = ?1
    WHERE membership.workspace_id = ?2
      AND membership.user_id = ?3
      AND membership.added_at_ms >= 0
      AND wg.status = 'active' AND wg.deactivated_at_ms IS NULL
      AND wg.member_count >= 0 AND wg.member_count <= 500
      AND wg.member_count = (
        SELECT COUNT(1) FROM (
          SELECT 1 FROM workspace_group_members identity_counted
          WHERE identity_counted.group_id = wg.id
            AND identity_counted.workspace_id = wg.workspace_id
          LIMIT 501
        )
      )
      AND 0 = (
        SELECT COUNT(1) FROM (
          SELECT user_id, workspace_id, added_at_ms
          FROM workspace_group_members identity_bounded
          WHERE identity_bounded.group_id = wg.id
            AND identity_bounded.workspace_id = wg.workspace_id
          LIMIT 501
        ) AS checked
        LEFT JOIN local_users checked_user
          ON checked_user.id = checked.user_id
         AND checked_user.workspace_id = checked.workspace_id
        WHERE checked.added_at_ms < 0
           OR checked_user.id IS NULL
           OR checked_user.status != 'active'
           OR checked_user.deactivated_at_ms IS NOT NULL
      )
      AND (
        SELECT COUNT(*) FROM (
          SELECT 1 FROM yard_access_grants bounded_grants
          WHERE bounded_grants.principal_kind = 'group'
            AND bounded_grants.principal_id = wg.id
            AND bounded_grants.status = 'active'
          LIMIT 501
        )
      ) <= 500
      AND grant.status = 'active' AND grant.revoked_at_ms IS NULL
      AND (grant.expires_at_ms IS NULL OR grant.expires_at_ms > ?5)
      AND (grant.environment_id IS NULL OR grant.environment_id = ?4)
    ORDER BY wg.id, grant.id";

pub(super) struct IdentityGrantSources {
    pub(super) groups: Vec<String>,
    pub(super) roles: Vec<String>,
}

pub(super) fn resolve(
    connection: &Connection,
    yard_id: &str,
    environment_id: &str,
    workspace_id: &str,
    user_id: &str,
    now_ms: i64,
) -> Result<IdentityGrantSources, RepositoryError> {
    for value in [yard_id, environment_id, workspace_id, user_id] {
        rows::validate_text(value)?;
    }
    require_bounded_memberships(connection, workspace_id, user_id)?;
    let mut roles = direct_roles(connection, yard_id, environment_id, user_id, now_ms)?;
    let group_rows = group_rows(
        connection,
        yard_id,
        environment_id,
        workspace_id,
        user_id,
        now_ms,
    )?;
    let mut groups = Vec::new();
    for (group_id, encoded_roles) in group_rows {
        if groups.last() != Some(&group_id) {
            groups.push(group_id);
        }
        roles
            .extend(yard_access::decode_roles(&encoded_roles).ok_or(RepositoryError::Unavailable)?);
    }
    Ok(IdentityGrantSources { groups, roles })
}

fn require_bounded_memberships(
    connection: &Connection,
    workspace_id: &str,
    user_id: &str,
) -> Result<(), RepositoryError> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM (
               SELECT 1 FROM workspace_group_members
               WHERE workspace_id = ?1 AND user_id = ?2 LIMIT 101
             )",
            params![workspace_id, user_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    (count <= i64::from(MAXIMUM_USER_GROUPS))
        .then_some(())
        .ok_or(RepositoryError::Unavailable)
}

fn direct_roles(
    connection: &Connection,
    yard_id: &str,
    environment_id: &str,
    user_id: &str,
    now_ms: i64,
) -> Result<Vec<String>, RepositoryError> {
    let mut statement = connection
        .prepare(
            "SELECT app_roles FROM yard_access_grants
             WHERE yard_id = ?1
               AND principal_kind = 'user' AND principal_id = ?2
               AND status = 'active' AND revoked_at_ms IS NULL
               AND (expires_at_ms IS NULL OR expires_at_ms > ?4)
               AND (environment_id IS NULL OR environment_id = ?3)
             ORDER BY id",
        )
        .map_err(map_error)?;
    let encoded = statement
        .query_map(params![yard_id, user_id, environment_id, now_ms], |row| {
            row.get::<_, String>(0)
        })
        .map_err(map_error)?;
    let mut roles = Vec::new();
    for value in encoded {
        roles.extend(
            yard_access::decode_roles(&value.map_err(map_error)?)
                .ok_or(RepositoryError::Unavailable)?,
        );
    }
    Ok(roles)
}

fn group_rows(
    connection: &Connection,
    yard_id: &str,
    environment_id: &str,
    workspace_id: &str,
    user_id: &str,
    now_ms: i64,
) -> Result<Vec<(String, String)>, RepositoryError> {
    let mut statement = connection.prepare(GROUP_ROLES_SQL).map_err(map_error)?;
    let rows = statement
        .query_map(
            params![yard_id, workspace_id, user_id, environment_id, now_ms],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(map_error)?;
    super::collect(rows)
}
