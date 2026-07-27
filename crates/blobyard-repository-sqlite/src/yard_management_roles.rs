use super::{
    lifecycle_audit, map_error, rows, yard_access, yard_management_role_rows, yard_validation,
};
use blobyard_contract::{
    AuditValue, MAXIMUM_YARD_MANAGEMENT_ROLES, NewAuditEvent, RepositoryError, YardManagementRole,
    YardManagementRoleAssignment, YardManagementRoleCursor, YardManagementRolePage,
};
use rusqlite::{Connection, OptionalExtension, Statement, Transaction, params};

use yard_management_role_rows::{COLUMNS, RoleState, assignment, role_state};

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
    let mut items = query_page(&mut statement, yard_id, precedence, user_id)?;
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

fn query_page(
    statement: &mut Statement<'_>,
    yard_id: &str,
    precedence: Option<u8>,
    user_id: Option<&str>,
) -> Result<Vec<YardManagementRoleAssignment>, RepositoryError> {
    super::collect(
        statement
            .query_map(params![yard_id, precedence, user_id], assignment)
            .map_err(map_error)?,
    )
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

fn validate_state(connection: &Connection, yard_id: &str) -> Result<RoleState, RepositoryError> {
    let state = connection
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(role = 'owner'), 0)
             FROM (
               SELECT role FROM yard_management_role_assignments
               WHERE yard_id = ?1 LIMIT 501
             )",
            [yard_id],
            role_state,
        )
        .map_err(map_error)?;
    let valid = state.assignments <= u64::from(MAXIMUM_YARD_MANAGEMENT_ROLES)
        && (state.assignments == 0 || state.owners > 0)
        && state.owners <= state.assignments;
    valid.then_some(state).ok_or(RepositoryError::Unavailable)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::query_page;
    use blobyard_contract::RepositoryError;
    use rusqlite::Connection;

    #[test]
    fn management_role_query_maps_parameter_failure() {
        let connection = Connection::open_in_memory().expect("connection");
        let mut statement = connection.prepare("SELECT 1").expect("wrong statement");
        assert_eq!(
            query_page(&mut statement, "yard", None, None),
            Err(RepositoryError::Unavailable)
        );
    }
}
