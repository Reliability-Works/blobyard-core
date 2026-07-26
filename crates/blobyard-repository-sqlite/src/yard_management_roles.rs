use super::{lifecycle_audit, map_error, rows, yard_access, yard_rows, yard_validation};
use blobyard_contract::{
    AuditValue, MAXIMUM_YARD_MANAGEMENT_ROLES, NewAuditEvent, RepositoryError, YardManagementRole,
    YardManagementRoleAssignment, YardManagementRoleCursor, YardManagementRolePage,
};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};

const COLUMNS: &str = "yard_id, user_id, workspace_id, role, created_at_ms, updated_at_ms";

pub(super) fn list(
    connection: &Connection,
    yard_id: &str,
    cursor: Option<&YardManagementRoleCursor>,
) -> Result<YardManagementRolePage, RepositoryError> {
    rows::validate_text(yard_id)?;
    require_active_yard(connection, yard_id)?;
    if let Some(cursor) = cursor {
        rows::validate_text(&cursor.user_id)?;
    }
    validate_state(connection, yard_id)?;
    let (precedence, user_id) = cursor.map_or((None, None), |value| {
        (Some(value.role.precedence()), Some(value.user_id.as_str()))
    });
    let mut statement = connection
        .prepare(&format!(
            "SELECT {COLUMNS} FROM yard_management_role_assignments
             WHERE yard_id = ?1
               AND (?2 IS NULL OR
                 CASE role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1
                   WHEN 'developer' THEN 2 ELSE 3 END > ?2
                 OR (CASE role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1
                   WHEN 'developer' THEN 2 ELSE 3 END = ?2 AND user_id > ?3))
             ORDER BY CASE role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1
               WHEN 'developer' THEN 2 ELSE 3 END, user_id
             LIMIT 51"
        ))
        .map_err(map_error)?;
    let rows = statement
        .query_map(params![yard_id, precedence, user_id], assignment)
        .map_err(map_error)?;
    let mut items = super::collect(rows)?;
    let next_cursor = (items.len() > 50).then(|| {
        let item = &items[49];
        YardManagementRoleCursor {
            role: item.role,
            user_id: item.user_id.clone(),
        }
    });
    items.truncate(50);
    Ok(YardManagementRolePage { items, next_cursor })
}

pub(super) fn set(
    transaction: &Transaction<'_>,
    yard_id: &str,
    user_id: &str,
    role: YardManagementRole,
    now_ms: i64,
    event: &NewAuditEvent,
) -> Result<YardManagementRoleAssignment, RepositoryError> {
    let yard = yard_access::active_yard(transaction, yard_id)?;
    require_active_user(transaction, &yard.workspace_id, user_id)?;
    let state = validate_state(transaction, yard_id)?;
    let previous = by_user(transaction, yard_id, user_id)?;
    if state.assignments == 0 && role != YardManagementRole::Owner {
        return Err(RepositoryError::Conflict);
    }
    if previous.is_none() && state.assignments >= u64::from(MAXIMUM_YARD_MANAGEMENT_ROLES) {
        return Err(RepositoryError::Conflict);
    }
    if previous
        .as_ref()
        .is_some_and(|value| value.role == YardManagementRole::Owner)
        && role != YardManagementRole::Owner
        && state.owners == 1
    {
        return Err(RepositoryError::Conflict);
    }
    let from = previous.as_ref().map(|value| value.role);
    validate_set_event(
        event,
        &yard.workspace_id,
        yard_id,
        user_id,
        from,
        role,
        now_ms,
    )?;
    transaction
        .execute(
            "INSERT INTO yard_management_role_assignments
               (yard_id, user_id, workspace_id, role, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(yard_id, user_id) DO UPDATE
               SET role = excluded.role, updated_at_ms = excluded.updated_at_ms",
            params![yard_id, user_id, yard.workspace_id, role.as_str(), now_ms],
        )
        .map_err(map_error)?;
    lifecycle_audit::insert(transaction, event)?;
    by_user(transaction, yard_id, user_id)?.ok_or(RepositoryError::Unavailable)
}

pub(super) fn revoke(
    transaction: &Transaction<'_>,
    yard_id: &str,
    user_id: &str,
    now_ms: i64,
    event: &NewAuditEvent,
) -> Result<(), RepositoryError> {
    let yard = yard_access::active_yard(transaction, yard_id)?;
    let state = validate_state(transaction, yard_id)?;
    let previous = by_user(transaction, yard_id, user_id)?.ok_or(RepositoryError::NotFound)?;
    if previous.role == YardManagementRole::Owner && state.owners == 1 {
        return Err(RepositoryError::Conflict);
    }
    yard_validation::action_event(
        event,
        "yard.management_role_revoked",
        "yard_management_role",
        &yard.workspace_id,
        u64::try_from(now_ms).map_err(|_error| RepositoryError::InvalidInput)?,
        [
            (
                "from",
                AuditValue::String(previous.role.as_str().to_owned()),
            ),
            ("userId", AuditValue::String(user_id.to_owned())),
            ("yardId", AuditValue::String(yard_id.to_owned())),
        ],
    )?;
    let changed = transaction
        .execute(
            "DELETE FROM yard_management_role_assignments WHERE yard_id = ?1 AND user_id = ?2",
            params![yard_id, user_id],
        )
        .map_err(map_error)?;
    super::changed_once(changed)?;
    lifecycle_audit::insert(transaction, event)
}

pub(super) fn by_user(
    connection: &Connection,
    yard_id: &str,
    user_id: &str,
) -> Result<Option<YardManagementRoleAssignment>, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {COLUMNS} FROM yard_management_role_assignments
                 WHERE yard_id = ?1 AND user_id = ?2"
            ),
            params![yard_id, user_id],
            assignment,
        )
        .optional()
        .map_err(map_error)
}

pub(super) fn require_owner(connection: &Connection, yard_id: &str) -> Result<(), RepositoryError> {
    let state = validate_state(connection, yard_id)?;
    (state.owners > 0)
        .then_some(())
        .ok_or(RepositoryError::Conflict)
}

pub(super) fn validate_integrity(
    connection: &Connection,
    yard_id: &str,
) -> Result<(), RepositoryError> {
    validate_state(connection, yard_id).map(|_state| ())
}

fn validate_set_event(
    event: &NewAuditEvent,
    workspace_id: &str,
    yard_id: &str,
    user_id: &str,
    from: Option<YardManagementRole>,
    to: YardManagementRole,
    now_ms: i64,
) -> Result<(), RepositoryError> {
    yard_validation::action_event(
        event,
        "yard.management_role_set",
        "yard_management_role",
        workspace_id,
        u64::try_from(now_ms).map_err(|_error| RepositoryError::InvalidInput)?,
        [
            (
                "from",
                from.map_or(AuditValue::Null, |role| {
                    AuditValue::String(role.as_str().to_owned())
                }),
            ),
            ("to", AuditValue::String(to.as_str().to_owned())),
            ("userId", AuditValue::String(user_id.to_owned())),
            ("yardId", AuditValue::String(yard_id.to_owned())),
        ],
    )
    .map(|_at| ())
}

fn require_active_yard(connection: &Connection, yard_id: &str) -> Result<(), RepositoryError> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM web_yards WHERE id = ?1 AND status = 'active')",
            [yard_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    exists.then_some(()).ok_or(RepositoryError::NotFound)
}

fn require_active_user(
    connection: &Connection,
    workspace_id: &str,
    user_id: &str,
) -> Result<(), RepositoryError> {
    rows::validate_text(user_id)?;
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM local_users
               WHERE id = ?1 AND workspace_id = ?2
                 AND status = 'active' AND deactivated_at_ms IS NULL
             )",
            params![user_id, workspace_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    exists.then_some(()).ok_or(RepositoryError::NotFound)
}

struct RoleState {
    assignments: u64,
    owners: u64,
}

fn validate_state(connection: &Connection, yard_id: &str) -> Result<RoleState, RepositoryError> {
    let state = connection
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(role = 'owner'), 0)
             FROM (
               SELECT role FROM yard_management_role_assignments
               WHERE yard_id = ?1 LIMIT 501
             )",
            [yard_id],
            |row| {
                Ok(RoleState {
                    assignments: yard_rows::required_u64(row.get(0)?)?,
                    owners: yard_rows::required_u64(row.get(1)?)?,
                })
            },
        )
        .map_err(map_error)?;
    let valid = state.assignments <= u64::from(MAXIMUM_YARD_MANAGEMENT_ROLES)
        && (state.assignments == 0 || state.owners > 0)
        && state.owners <= state.assignments;
    valid.then_some(state).ok_or(RepositoryError::Unavailable)
}

fn assignment(row: &Row<'_>) -> rusqlite::Result<YardManagementRoleAssignment> {
    let role: String = row.get(3)?;
    let assignment = YardManagementRoleAssignment {
        yard_id: row.get(0)?,
        user_id: row.get(1)?,
        workspace_id: row.get(2)?,
        role: YardManagementRole::parse(&role).ok_or_else(|| rows::conversion_error(role))?,
        created_at_ms: yard_rows::required_u64(row.get(4)?)?,
        updated_at_ms: yard_rows::required_u64(row.get(5)?)?,
    };
    let valid = rows::validate_text(&assignment.yard_id).is_ok()
        && rows::validate_text(&assignment.user_id).is_ok()
        && rows::validate_text(&assignment.workspace_id).is_ok()
        && assignment.updated_at_ms >= assignment.created_at_ms;
    valid
        .then_some(assignment)
        .ok_or_else(|| rows::conversion_error("invalid persisted management role"))
}
