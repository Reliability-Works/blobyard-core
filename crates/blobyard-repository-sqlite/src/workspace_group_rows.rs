use super::{rows, yard_rows};
use blobyard_contract::{
    MAXIMUM_GROUP_MEMBERS, RepositoryError, WorkspaceGroupMemberRecord, WorkspaceGroupRecord,
    WorkspaceGroupStatus, normalize_group_name,
};
use rusqlite::Row;

pub(super) const QUALIFIED_GROUP_COLUMNS: &str =
    "g.id, g.workspace_id, g.name, g.status, g.member_count, g.created_at_ms, g.deactivated_at_ms";
pub(super) const QUALIFIED_MEMBER_COLUMNS: &str =
    "m.group_id, m.workspace_id, m.user_id, m.added_at_ms";

pub(super) fn group(row: &Row<'_>) -> rusqlite::Result<WorkspaceGroupRecord> {
    let status: String = row.get(3)?;
    let member_count = yard_rows::required_u64(row.get(4)?)?;
    let group = WorkspaceGroupRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        name: row.get(2)?,
        status: WorkspaceGroupStatus::parse(&status)
            .ok_or_else(|| rows::conversion_error(status))?,
        member_count: u32::try_from(member_count)
            .map_err(|error| rows::conversion_error(error.to_string()))?,
        created_at_ms: yard_rows::required_u64(row.get(5)?)?,
        deactivated_at_ms: row
            .get::<_, Option<i64>>(6)?
            .map(yard_rows::required_u64)
            .transpose()?,
    };
    validate_stored_group(&group)?;
    Ok(group)
}

pub(super) fn member(row: &Row<'_>) -> rusqlite::Result<WorkspaceGroupMemberRecord> {
    let member = WorkspaceGroupMemberRecord {
        group_id: row.get(0)?,
        workspace_id: row.get(1)?,
        user_id: row.get(2)?,
        added_at_ms: yard_rows::required_u64(row.get(3)?)?,
    };
    validate_member(&member)
        .map_err(|_error| rows::conversion_error("invalid persisted member"))?;
    Ok(member)
}

fn validate_stored_group(group: &WorkspaceGroupRecord) -> rusqlite::Result<()> {
    let valid_identity = validate_group_id(&group.id).is_ok()
        && rows::validate_text(&group.workspace_id).is_ok()
        && normalize_group_name(&group.name).is_ok_and(|name| name == group.name)
        && group.member_count <= MAXIMUM_GROUP_MEMBERS;
    let valid_lifecycle = match group.status {
        WorkspaceGroupStatus::Active => group.deactivated_at_ms.is_none(),
        WorkspaceGroupStatus::Deactivated => {
            group.member_count == 0
                && group
                    .deactivated_at_ms
                    .is_some_and(|at_ms| at_ms >= group.created_at_ms)
        }
    };
    (valid_identity && valid_lifecycle)
        .then_some(())
        .ok_or_else(|| rows::conversion_error("invalid persisted workspace group"))
}

pub(super) fn validate_group_id(value: &str) -> Result<(), RepositoryError> {
    if rows::valid_prefixed_hex_id(value, "group_") {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

pub(super) fn validate_group(group: &WorkspaceGroupRecord) -> Result<(), RepositoryError> {
    validate_group_id(&group.id)?;
    rows::validate_text(&group.workspace_id)?;
    let name = normalize_group_name(&group.name)?;
    let valid = name == group.name
        && group.status == WorkspaceGroupStatus::Active
        && group.member_count == 0
        && group.deactivated_at_ms.is_none();
    if valid {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

pub(super) fn validate_member(member: &WorkspaceGroupMemberRecord) -> Result<(), RepositoryError> {
    validate_group_id(&member.group_id)?;
    rows::validate_text(&member.workspace_id)?;
    rows::validate_text(&member.user_id)
}
