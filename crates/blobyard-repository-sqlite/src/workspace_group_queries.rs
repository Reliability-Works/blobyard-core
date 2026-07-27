use super::{map_error, rows, workspace_group_rows};
use blobyard_contract::{
    RepositoryError, WorkspaceGroupCursor, WorkspaceGroupMemberCursor, WorkspaceGroupMemberPage,
    WorkspaceGroupPage, WorkspaceGroupRecord, WorkspaceGroupStatus,
};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) fn map_query_result<T>(result: rusqlite::Result<T>) -> Result<T, RepositoryError> {
    result.map_err(map_error)
}

pub(super) fn by_id(
    connection: &Connection,
    workspace_id: &str,
    group_id: &str,
) -> Result<Option<WorkspaceGroupRecord>, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM workspace_groups g JOIN workspaces w ON w.id = g.workspace_id WHERE g.id = ?1 AND g.workspace_id = ?2",
                workspace_group_rows::QUALIFIED_GROUP_COLUMNS
            ),
            params![group_id, workspace_id],
            workspace_group_rows::group,
        )
        .optional()
        .map_err(map_error)
}

pub(super) fn active(
    connection: &Connection,
    workspace_id: &str,
    group_id: &str,
) -> Result<WorkspaceGroupRecord, RepositoryError> {
    let group = by_id(connection, workspace_id, group_id)?.ok_or(RepositoryError::NotFound)?;
    if group.status == WorkspaceGroupStatus::Active {
        Ok(group)
    } else {
        Err(RepositoryError::Conflict)
    }
}

pub(super) fn list_groups(
    connection: &Connection,
    workspace_id: &str,
    cursor: Option<&WorkspaceGroupCursor>,
    limit: u32,
) -> Result<WorkspaceGroupPage, RepositoryError> {
    rows::validate_text(workspace_id)?;
    validate_limit(limit)?;
    if let Some(cursor) = cursor {
        workspace_group_rows::validate_group_id(&cursor.id)?;
    }
    let (at, id) = cursor.map_or((None, None), |value| {
        (
            i64::try_from(value.created_at_ms).ok(),
            Some(value.id.as_str()),
        )
    });
    if cursor.is_some() && at.is_none() {
        return Err(RepositoryError::InvalidInput);
    }
    let mut statement = connection
        .prepare(&format!(
            "SELECT {} FROM workspace_groups g JOIN workspaces w ON w.id = g.workspace_id WHERE g.workspace_id = ?1 AND (?2 IS NULL OR g.created_at_ms < ?2 OR (g.created_at_ms = ?2 AND g.id < ?3)) ORDER BY g.created_at_ms DESC, g.id DESC LIMIT ?4",
            workspace_group_rows::QUALIFIED_GROUP_COLUMNS
        ))
        .map_err(map_error)?;
    let rows = statement.query_map(
        params![workspace_id, at, id, i64::from(limit) + 1],
        workspace_group_rows::group,
    );
    map_query_result(rows)
        .and_then(super::collect)
        .map(|items| group_page(items, limit))
}

pub(super) fn list_members(
    connection: &Connection,
    workspace_id: &str,
    group_id: &str,
    cursor: Option<&WorkspaceGroupMemberCursor>,
    limit: u32,
) -> Result<WorkspaceGroupMemberPage, RepositoryError> {
    rows::validate_text(workspace_id)?;
    workspace_group_rows::validate_group_id(group_id)?;
    validate_limit(limit)?;
    active(connection, workspace_id, group_id)?;
    let (at, user_id) = cursor.map_or((None, None), |value| {
        (
            i64::try_from(value.added_at_ms).ok(),
            Some(value.user_id.as_str()),
        )
    });
    if cursor.is_some() && at.is_none() {
        return Err(RepositoryError::InvalidInput);
    }
    let mut statement = connection
        .prepare(&format!(
            "SELECT {} FROM workspace_group_members m JOIN local_users u ON u.id = m.user_id AND u.workspace_id = m.workspace_id AND u.status = 'active' WHERE m.group_id = ?1 AND m.workspace_id = ?2 AND (?3 IS NULL OR m.added_at_ms < ?3 OR (m.added_at_ms = ?3 AND m.user_id < ?4)) ORDER BY m.added_at_ms DESC, m.user_id DESC LIMIT ?5",
            workspace_group_rows::QUALIFIED_MEMBER_COLUMNS
        ))
        .map_err(map_error)?;
    let rows = statement.query_map(
        params![group_id, workspace_id, at, user_id, i64::from(limit) + 1],
        workspace_group_rows::member,
    );
    map_query_result(rows)
        .and_then(super::collect)
        .map(|items| member_page(items, limit))
}

fn group_page(mut items: Vec<WorkspaceGroupRecord>, limit: u32) -> WorkspaceGroupPage {
    let has_more = items.len() > limit as usize;
    let next_cursor = if has_more {
        let group = &items[limit as usize - 1];
        Some(WorkspaceGroupCursor {
            created_at_ms: group.created_at_ms,
            id: group.id.clone(),
        })
    } else {
        None
    };
    items.truncate(limit as usize);
    WorkspaceGroupPage { items, next_cursor }
}

fn member_page(
    mut items: Vec<blobyard_contract::WorkspaceGroupMemberRecord>,
    limit: u32,
) -> WorkspaceGroupMemberPage {
    let has_more = items.len() > limit as usize;
    let next_cursor = if has_more {
        let member = &items[limit as usize - 1];
        Some(WorkspaceGroupMemberCursor {
            added_at_ms: member.added_at_ms,
            user_id: member.user_id.clone(),
        })
    } else {
        None
    };
    items.truncate(limit as usize);
    WorkspaceGroupMemberPage { items, next_cursor }
}

fn validate_limit(limit: u32) -> Result<(), RepositoryError> {
    if (1..=50).contains(&limit) {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}
