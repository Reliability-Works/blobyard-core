use super::{map_error, rows};
use blobyard_contract::{NewAuditEvent, RepositoryError};
use rusqlite::{Connection, Statement, Transaction, params};

pub(super) struct Operation {
    pub(super) id: String,
    pub(super) project_id: String,
    pub(super) object_path: String,
    pub(super) selected_version: Option<i64>,
    pub(super) reason: String,
    pub(super) actor: String,
    pub(super) request_id: String,
    pub(super) complete: bool,
}

pub(super) fn operation(connection: &Connection, id: &str) -> Result<Operation, RepositoryError> {
    connection
        .query_row(
            "SELECT id, project_id, object_path, selected_version, reason, actor, request_id, status = 'complete' FROM deletion_operations WHERE id = ?1",
            [id],
            operation_row,
        )
        .map_err(map_error)
}

pub(super) fn validate_event(
    connection: &Connection,
    operation: &Operation,
    event: &NewAuditEvent,
) -> Result<(), RepositoryError> {
    let action = match operation.reason.as_str() {
        "retention" => "retention.enforced",
        "yard_cleanup" => "yard.cleanup_completed",
        _ => "object.deleted",
    };
    let workspace_id: String = connection
        .query_row(
            "SELECT workspace_id FROM projects WHERE id = ?1",
            [&operation.project_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    if operation.actor == event.actor
        && operation.request_id == event.request_id
        && workspace_id == event.workspace_id
        && event.action == action
    {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

pub(super) fn operation_version_ids(
    connection: &Connection,
    id: &str,
) -> Result<Vec<String>, RepositoryError> {
    let mut statement = connection
        .prepare(
            "SELECT version_id FROM deletion_items WHERE operation_id = ?1 ORDER BY version_id",
        )
        .map_err(map_error)?;
    query_operation_version_ids(&mut statement, id)
}

fn query_operation_version_ids(
    statement: &mut Statement<'_>,
    id: &str,
) -> Result<Vec<String>, RepositoryError> {
    statement
        .query_map([id], |row| row.get(0))
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)
}

pub(super) fn delete_aborted_target(
    transaction: &Transaction<'_>,
    operation: &Operation,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "DELETE FROM upload_reservations WHERE version_id IN (SELECT id FROM object_versions WHERE project_id = ?1 AND object_path = ?2 AND state = 'aborted' AND (?3 IS NULL OR version = ?3))",
            params![operation.project_id, operation.object_path, operation.selected_version],
        )
        .map_err(map_error)?;
    transaction
        .execute(
            "DELETE FROM object_versions WHERE project_id = ?1 AND object_path = ?2 AND state = 'aborted' AND (?3 IS NULL OR version = ?3)",
            params![operation.project_id, operation.object_path, operation.selected_version],
        )
        .map_err(map_error)?;
    Ok(())
}

pub(super) fn complete_run(
    transaction: &Transaction<'_>,
    operation: &Operation,
    completed_at_ms: i64,
) -> Result<(), RepositoryError> {
    if operation.reason != "retention" {
        return Ok(());
    }
    transaction
        .execute(
            "UPDATE retention_runs SET deleted_count = (SELECT COUNT(*) FROM deletion_items WHERE operation_id = ?1), status = 'complete', completed_at_ms = ?2, error_summary = NULL WHERE id = ?1",
            params![operation.id, completed_at_ms],
        )
        .map_err(map_error)?;
    Ok(())
}

fn operation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Operation> {
    let selected: Option<i64> = row.get(3)?;
    Ok(Operation {
        id: row.get(0)?,
        project_id: row.get(1)?,
        object_path: row.get(2)?,
        selected_version: selected
            .map(|value| {
                u64::try_from(value)
                    .map(|_| value)
                    .map_err(rows::conversion_error)
            })
            .transpose()?,
        reason: row.get(4)?,
        actor: row.get(5)?,
        request_id: row.get(6)?,
        complete: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::{operation_row, query_operation_version_ids};
    use blobyard_contract::RepositoryError;
    use rusqlite::Connection;

    #[test]
    fn operation_rows_reject_each_malformed_provider_field() {
        let connection = Connection::open_in_memory().expect("connection");
        for query in [
            "SELECT 1, 'project', 'path', NULL, 'reason', 'actor', 'request', 0",
            "SELECT 'id', 1, 'path', NULL, 'reason', 'actor', 'request', 0",
            "SELECT 'id', 'project', 1, NULL, 'reason', 'actor', 'request', 0",
            "SELECT 'id', 'project', 'path', 'bad', 'reason', 'actor', 'request', 0",
            "SELECT 'id', 'project', 'path', NULL, 1, 'actor', 'request', 0",
            "SELECT 'id', 'project', 'path', NULL, 'reason', 1, 'request', 0",
            "SELECT 'id', 'project', 'path', NULL, 'reason', 'actor', 1, 0",
            "SELECT 'id', 'project', 'path', NULL, 'reason', 'actor', 'request', 'bad'",
            "SELECT 'id', 'project', 'path', -1, 'reason', 'actor', 'request', 0",
        ] {
            assert!(connection.query_row(query, [], operation_row).is_err());
        }
    }

    #[test]
    fn operation_item_query_maps_parameter_failure() {
        let connection = Connection::open_in_memory().expect("connection");
        let mut statement = connection.prepare("SELECT 1").expect("wrong statement");
        assert_eq!(
            query_operation_version_ids(&mut statement, "operation"),
            Err(RepositoryError::Unavailable)
        );
    }
}
