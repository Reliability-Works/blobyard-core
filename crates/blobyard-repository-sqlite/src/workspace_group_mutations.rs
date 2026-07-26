use super::{
    map_error, workspace_group_audit, workspace_group_queries, workspace_group_rows,
    workspace_group_validation,
};
use blobyard_contract::{
    AuditValue, MAXIMUM_ACTIVE_GROUP_GRANTS, MAXIMUM_ACTIVE_GROUPS, NewAuditEvent, RepositoryError,
    WorkspaceGroupRecord, normalize_group_name,
};
use rusqlite::{Transaction, params};

pub(super) fn create(
    transaction: &Transaction<'_>,
    group: &WorkspaceGroupRecord,
    event: &NewAuditEvent,
) -> Result<(), RepositoryError> {
    workspace_group_rows::validate_group(group)?;
    workspace_group_validation::require_workspace(transaction, &group.workspace_id)?;
    workspace_group_validation::require_below_limit(
        transaction,
        "SELECT COUNT(*) FROM workspace_groups WHERE workspace_id = ?1 AND status = 'active'",
        &group.workspace_id,
        MAXIMUM_ACTIVE_GROUPS,
    )?;
    workspace_group_validation::require_unresolved_grant_absent(transaction, &group.id)?;
    let created_at = workspace_group_audit::group_event(
        event,
        "group.created",
        group,
        group.created_at_ms,
        [("name", AuditValue::String(group.name.clone()))],
    )?;
    transaction
        .execute(
            "INSERT INTO workspace_groups (id, workspace_id, name, status, member_count, created_at_ms, deactivated_at_ms) VALUES (?1, ?2, ?3, 'active', 0, ?4, NULL)",
            params![group.id, group.workspace_id, group.name, created_at],
        )
        .map_err(map_error)?;
    super::lifecycle_audit::insert(transaction, event)
}

pub(super) fn rename(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    group_id: &str,
    name: &str,
    event: &NewAuditEvent,
) -> Result<WorkspaceGroupRecord, RepositoryError> {
    workspace_group_rows::validate_group_id(group_id)?;
    let name = normalize_group_name(name)?;
    let group = workspace_group_queries::active(transaction, workspace_id, group_id)?;
    workspace_group_audit::validate_rename_event(event, &group, &name)?;
    transaction
        .execute(
            "UPDATE workspace_groups SET name = ?3 WHERE id = ?1 AND workspace_id = ?2 AND status = 'active'",
            params![group_id, workspace_id, name],
        )
        .map_err(map_error)?;
    workspace_group_audit::insert_rename_event(transaction, event, &group.name)?;
    workspace_group_queries::by_id(transaction, workspace_id, group_id)?
        .ok_or(RepositoryError::Unavailable)
}

pub(super) fn deactivate(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    group_id: &str,
    now_ms: u64,
    event: &NewAuditEvent,
) -> Result<(), RepositoryError> {
    workspace_group_rows::validate_group_id(group_id)?;
    let group = workspace_group_queries::active(transaction, workspace_id, group_id)?;
    let now = workspace_group_audit::validate_deactivate_event(event, &group, now_ms)?;
    let revoked = workspace_group_validation::active_grant_count(transaction, group_id)?;
    if revoked > u64::from(MAXIMUM_ACTIVE_GROUP_GRANTS) {
        return Err(RepositoryError::Conflict);
    }
    transaction
        .execute(
            "UPDATE yard_access_grants SET status = 'revoked', revoked_at_ms = ?2 WHERE principal_kind = 'group' AND principal_id = ?1 AND status = 'active'",
            params![group_id, now],
        )
        .map_err(map_error)?;
    transaction
        .execute(
            "DELETE FROM workspace_group_members WHERE group_id = ?1 AND workspace_id = ?2",
            params![group_id, workspace_id],
        )
        .map_err(map_error)?;
    transaction
        .execute(
            "UPDATE workspace_groups SET status = 'deactivated', member_count = 0, deactivated_at_ms = ?3 WHERE id = ?1 AND workspace_id = ?2 AND status = 'active'",
            params![group_id, workspace_id, now],
        )
        .map_err(map_error)?;
    workspace_group_audit::insert_deactivate_event(transaction, event, revoked)
}
