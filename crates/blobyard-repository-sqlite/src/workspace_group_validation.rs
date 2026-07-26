use super::map_error;
use blobyard_contract::RepositoryError;
use rusqlite::{Connection, params};

pub(super) fn require_workspace(
    connection: &Connection,
    workspace_id: &str,
) -> Result<(), RepositoryError> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?1)",
            [workspace_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    exists.then_some(()).ok_or(RepositoryError::NotFound)
}

pub(super) fn require_active_user(
    connection: &Connection,
    workspace_id: &str,
    user_id: &str,
) -> Result<(), RepositoryError> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM local_users WHERE id = ?1 AND workspace_id = ?2 AND status = 'active')",
            params![user_id, workspace_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    exists.then_some(()).ok_or(RepositoryError::NotFound)
}

pub(super) fn require_below_limit(
    connection: &Connection,
    sql: &str,
    value: &str,
    maximum: u32,
) -> Result<(), RepositoryError> {
    let count: i64 = connection
        .query_row(sql, [value], |row| row.get(0))
        .map_err(map_error)?;
    (count < i64::from(maximum))
        .then_some(())
        .ok_or(RepositoryError::Conflict)
}

pub(super) fn require_unresolved_grant_absent(
    connection: &Connection,
    group_id: &str,
) -> Result<(), RepositoryError> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM yard_access_grants WHERE principal_kind = 'group' AND principal_id = ?1)",
            [group_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    (!exists).then_some(()).ok_or(RepositoryError::Conflict)
}

pub(super) fn active_grant_count(
    connection: &Connection,
    group_id: &str,
) -> Result<u64, RepositoryError> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM yard_access_grants WHERE principal_kind = 'group' AND principal_id = ?1 AND status = 'active'",
            [group_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    u64::try_from(count).map_err(|_error| RepositoryError::Unavailable)
}
