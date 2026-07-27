use super::{rows, yard_rows};
use blobyard_contract::{YardManagementRole, YardManagementRoleAssignment};
use rusqlite::Row;

pub(super) const COLUMNS: &str =
    "yard_id, user_id, workspace_id, role, created_at_ms, updated_at_ms";

pub(super) struct RoleState {
    pub(super) assignments: u64,
    pub(super) owners: u64,
}

pub(super) fn role_state(row: &Row<'_>) -> rusqlite::Result<RoleState> {
    Ok(RoleState {
        assignments: yard_rows::required_u64(row.get(0)?)?,
        owners: yard_rows::required_u64(row.get(1)?)?,
    })
}

pub(super) fn assignment(row: &Row<'_>) -> rusqlite::Result<YardManagementRoleAssignment> {
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::{assignment, role_state};
    use blobyard_contract::YardManagementRoleAssignment;
    use rusqlite::{Connection, params_from_iter, types::Value};

    fn assignment_result(values: Vec<Value>) -> rusqlite::Result<YardManagementRoleAssignment> {
        Connection::open_in_memory().expect("connection").query_row(
            "SELECT ?1, ?2, ?3, ?4, ?5, ?6",
            params_from_iter(values),
            assignment,
        )
    }

    fn valid_assignment() -> Vec<Value> {
        vec![
            Value::Text("yard".to_owned()),
            Value::Text("user".to_owned()),
            Value::Text("workspace".to_owned()),
            Value::Text("owner".to_owned()),
            Value::Integer(1),
            Value::Integer(1),
        ]
    }

    #[test]
    fn assignment_row_rejects_every_invalid_column() {
        assert!(assignment_result(valid_assignment()).is_ok());
        for (index, value) in [
            (0, Value::Integer(1)),
            (0, Value::Text(String::new())),
            (1, Value::Integer(1)),
            (1, Value::Text(String::new())),
            (2, Value::Integer(1)),
            (2, Value::Text(String::new())),
            (3, Value::Integer(1)),
            (3, Value::Text("invalid".to_owned())),
            (4, Value::Text("bad".to_owned())),
            (4, Value::Integer(-1)),
            (5, Value::Text("bad".to_owned())),
            (5, Value::Integer(-1)),
        ] {
            let mut values = valid_assignment();
            values[index] = value;
            assert!(assignment_result(values).is_err(), "column {index}");
        }
        let mut backwards = valid_assignment();
        backwards[4] = Value::Integer(2);
        assert!(assignment_result(backwards).is_err());
    }

    #[test]
    fn role_state_row_rejects_non_unsigned_counts() {
        let connection = Connection::open_in_memory().expect("connection");
        for (assignments, owners, valid) in [
            (Value::Integer(1), Value::Integer(1), true),
            (Value::Text("bad".to_owned()), Value::Integer(1), false),
            (Value::Integer(-1), Value::Integer(1), false),
            (Value::Integer(1), Value::Text("bad".to_owned()), false),
            (Value::Integer(1), Value::Integer(-1), false),
        ] {
            let result = connection.query_row("SELECT ?1, ?2", [assignments, owners], role_state);
            assert_eq!(result.is_ok(), valid);
        }
    }
}
