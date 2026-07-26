use super::{
    lifecycle_audit, map_error, rows, workspace_group_audit, workspace_group_queries,
    workspace_group_rows, workspace_group_validation,
};
use blobyard_contract::{
    MAXIMUM_GROUP_MEMBERS, MAXIMUM_USER_GROUPS, NewAuditEvent, RepositoryError,
    WorkspaceGroupMemberRecord,
};
use rusqlite::{Transaction, params};

pub(super) fn add(
    transaction: &Transaction<'_>,
    member: &WorkspaceGroupMemberRecord,
    event: &NewAuditEvent,
) -> Result<(), RepositoryError> {
    workspace_group_rows::validate_member(member)?;
    let group =
        workspace_group_queries::active(transaction, &member.workspace_id, &member.group_id)?;
    workspace_group_validation::require_active_user(
        transaction,
        &member.workspace_id,
        &member.user_id,
    )?;
    workspace_group_validation::require_below_limit(
        transaction,
        "SELECT member_count FROM workspace_groups WHERE id = ?1",
        &member.group_id,
        MAXIMUM_GROUP_MEMBERS,
    )?;
    workspace_group_validation::require_below_limit(
        transaction,
        "SELECT COUNT(*) FROM workspace_group_members WHERE user_id = ?1",
        &member.user_id,
        MAXIMUM_USER_GROUPS,
    )?;
    let added_at = workspace_group_audit::member_event(
        event,
        "group.member_added",
        &group,
        &member.user_id,
        member.added_at_ms,
    )?;
    transaction
        .execute(
            "INSERT INTO workspace_group_members (group_id, workspace_id, user_id, added_at_ms) VALUES (?1, ?2, ?3, ?4)",
            params![
                member.group_id,
                member.workspace_id,
                member.user_id,
                added_at,
            ],
        )
        .map_err(map_error)?;
    increment_member_count(transaction, &member.group_id, 1)?;
    lifecycle_audit::insert(transaction, event)
}

pub(super) fn remove(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    group_id: &str,
    user_id: &str,
    event: &NewAuditEvent,
) -> Result<(), RepositoryError> {
    workspace_group_rows::validate_group_id(group_id)?;
    rows::validate_text(user_id)?;
    let group = workspace_group_queries::active(transaction, workspace_id, group_id)?;
    let _event_at = workspace_group_audit::member_event(
        event,
        "group.member_removed",
        &group,
        user_id,
        event.created_at_ms,
    )?;
    let changed = transaction
        .execute(
            "DELETE FROM workspace_group_members WHERE group_id = ?1 AND workspace_id = ?2 AND user_id = ?3",
            params![group_id, workspace_id, user_id],
        )
        .map_err(map_error)?;
    if changed != 1 {
        return Err(RepositoryError::NotFound);
    }
    increment_member_count(transaction, group_id, -1)?;
    lifecycle_audit::insert(transaction, event)
}

pub(super) fn remove_user_memberships(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    user_id: &str,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "UPDATE workspace_groups
             SET member_count = member_count - 1
             WHERE id IN (
                 SELECT group_id FROM workspace_group_members
                 WHERE workspace_id = ?1 AND user_id = ?2
             )",
            params![workspace_id, user_id],
        )
        .map_err(map_error)?;
    transaction
        .execute(
            "DELETE FROM workspace_group_members WHERE workspace_id = ?1 AND user_id = ?2",
            params![workspace_id, user_id],
        )
        .map_err(map_error)?;
    Ok(())
}

fn increment_member_count(
    transaction: &Transaction<'_>,
    group_id: &str,
    delta: i64,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "UPDATE workspace_groups SET member_count = member_count + ?2 WHERE id = ?1",
            params![group_id, delta],
        )
        .map(|_changed| ())
        .map_err(map_error)
}
