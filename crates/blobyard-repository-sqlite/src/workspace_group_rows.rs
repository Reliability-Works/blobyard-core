use super::{rows, yard_rows};
use blobyard_contract::{
    RepositoryError, WorkspaceGroupMemberRecord, WorkspaceGroupRecord, WorkspaceGroupStatus,
    normalize_group_name,
};
use rusqlite::Row;

pub(super) const GROUP_COLUMNS: &str =
    "id, workspace_id, name, status, member_count, created_at_ms, deactivated_at_ms";
pub(super) const MEMBER_COLUMNS: &str = "group_id, workspace_id, user_id, added_at_ms";

pub(super) fn group(row: &Row<'_>) -> rusqlite::Result<WorkspaceGroupRecord> {
    let status: String = row.get(3)?;
    let member_count = yard_rows::required_u64(row.get(4)?)?;
    Ok(WorkspaceGroupRecord {
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
    })
}

pub(super) fn member(row: &Row<'_>) -> rusqlite::Result<WorkspaceGroupMemberRecord> {
    Ok(WorkspaceGroupMemberRecord {
        group_id: row.get(0)?,
        workspace_id: row.get(1)?,
        user_id: row.get(2)?,
        added_at_ms: yard_rows::required_u64(row.get(3)?)?,
    })
}

pub(super) fn validate_group_id(value: &str) -> Result<(), RepositoryError> {
    let suffix = value.strip_prefix("group_");
    let valid = suffix.is_some_and(|suffix| {
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    });
    if valid {
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
