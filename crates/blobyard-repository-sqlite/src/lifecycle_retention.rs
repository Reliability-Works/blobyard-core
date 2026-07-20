use super::{lifecycle_audit, lifecycle_audit::to_i64, map_error, rows};
use blobyard_contract::{
    NewAuditEvent, RepositoryError, RetentionOverview, RetentionPolicyRecord, RetentionRunRecord,
};
use rusqlite::{Connection, OptionalExtension, Statement, Transaction, params};

pub(super) fn policy(
    connection: &Connection,
    project_id: &str,
) -> Result<RetentionPolicyRecord, RepositoryError> {
    rows::validate_text(project_id)?;
    connection
        .query_row(
            "SELECT project_id, keep_latest, path_glob, branch_glob, created_at_ms, updated_at_ms FROM retention_policies WHERE project_id = ?1 AND enabled = 1",
            [project_id],
            policy_row,
        )
        .map_err(map_error)
}

pub(super) fn set(
    transaction: &Transaction<'_>,
    policy: &RetentionPolicyRecord,
    event: &NewAuditEvent,
) -> Result<(), RepositoryError> {
    validate_policy(policy)?;
    assert_project_event(
        transaction,
        &policy.project_id,
        event,
        "retention.policy_set",
    )?;
    assert_no_pending(transaction, &policy.project_id)?;
    transaction
        .execute(
            "INSERT INTO retention_policies (project_id, keep_latest, path_glob, branch_glob, enabled, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6) ON CONFLICT(project_id) DO UPDATE SET keep_latest = excluded.keep_latest, path_glob = excluded.path_glob, branch_glob = excluded.branch_glob, enabled = 1, updated_at_ms = excluded.updated_at_ms",
            params![
                policy.project_id,
                i64::from(policy.keep_latest),
                policy.path_glob,
                policy.branch_glob,
                to_i64(policy.created_at_ms)?,
                to_i64(policy.updated_at_ms)?,
            ],
        )
        .map_err(map_error)?;
    lifecycle_audit::insert(transaction, event)
}

pub(super) fn clear(
    transaction: &Transaction<'_>,
    project_id: &str,
    updated_at_ms: u64,
    event: &NewAuditEvent,
) -> Result<bool, RepositoryError> {
    rows::validate_text(project_id)?;
    assert_project_event(transaction, project_id, event, "retention.policy_cleared")?;
    assert_no_pending(transaction, project_id)?;
    let changed = transaction
        .execute(
            "UPDATE retention_policies SET enabled = 0, updated_at_ms = ?2 WHERE project_id = ?1 AND enabled = 1",
            params![project_id, to_i64(updated_at_ms)?],
        )
        .map_err(map_error)?;
    if changed == 1 {
        lifecycle_audit::insert(transaction, event)?;
    }
    Ok(changed == 1)
}

pub(super) fn overview(
    connection: &Connection,
    project_id: &str,
) -> Result<RetentionOverview, RepositoryError> {
    rows::validate_text(project_id)?;
    ensure_project(connection, project_id)?;
    let policy = match policy(connection, project_id) {
        Ok(value) => Some(value),
        Err(RepositoryError::NotFound) => None,
        Err(error) => return Err(error),
    };
    let last_run = connection
        .query_row(
            "SELECT id, candidate_count, deleted_count, status, started_at_ms, completed_at_ms, error_summary FROM retention_runs WHERE project_id = ?1 ORDER BY started_at_ms DESC, id DESC LIMIT 1",
            [project_id],
            run_row,
        )
        .optional()
        .map_err(map_error)?;
    Ok(RetentionOverview { policy, last_run })
}

pub(super) fn fail(
    connection: &Connection,
    run_id: &str,
    completed_at_ms: u64,
) -> Result<(), RepositoryError> {
    rows::validate_text(run_id)?;
    let changed = connection
        .execute(
            "UPDATE retention_runs SET status = 'failed', completed_at_ms = ?2, error_summary = 'Storage deletion did not complete.' WHERE id = ?1 AND status != 'complete'",
            params![run_id, to_i64(completed_at_ms)?],
        )
        .map_err(map_error)?;
    if changed == 1 {
        Ok(())
    } else {
        Err(RepositoryError::NotFound)
    }
}

pub(super) fn projects(connection: &Connection) -> Result<Vec<String>, RepositoryError> {
    let mut statement = connection
        .prepare("SELECT project_id FROM retention_policies WHERE enabled = 1 ORDER BY project_id")
        .map_err(map_error)?;
    query_projects(&mut statement)
}

fn query_projects(statement: &mut Statement<'_>) -> Result<Vec<String>, RepositoryError> {
    statement
        .raw_query()
        .mapped(project_id)
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)
}

fn project_id(row: &rusqlite::Row<'_>) -> rusqlite::Result<String> {
    row.get(0)
}

fn assert_project_event(
    connection: &Connection,
    project_id: &str,
    event: &NewAuditEvent,
    action: &str,
) -> Result<(), RepositoryError> {
    let workspace_id: String = connection
        .query_row(
            "SELECT workspace_id FROM projects WHERE id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    if workspace_id == event.workspace_id && event.action == action {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

fn assert_no_pending(connection: &Connection, project_id: &str) -> Result<(), RepositoryError> {
    let pending: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM deletion_operations WHERE project_id = ?1 AND reason = 'retention' AND status = 'pending')",
            [project_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    if pending {
        Err(RepositoryError::Conflict)
    } else {
        Ok(())
    }
}

fn ensure_project(connection: &Connection, project_id: &str) -> Result<(), RepositoryError> {
    connection
        .query_row(
            "SELECT id FROM projects WHERE id = ?1",
            [project_id],
            |row| row.get::<_, String>(0),
        )
        .map(|_id| ())
        .map_err(map_error)
}

fn validate_policy(policy: &RetentionPolicyRecord) -> Result<(), RepositoryError> {
    rows::validate_text(&policy.project_id)?;
    if policy.keep_latest == 0 || policy.updated_at_ms < policy.created_at_ms {
        return Err(RepositoryError::InvalidInput);
    }
    validate_glob(policy.path_glob.as_deref(), true)?;
    validate_glob(policy.branch_glob.as_deref(), false)
}

fn validate_glob(value: Option<&str>, path: bool) -> Result<(), RepositoryError> {
    let Some(value) = value else {
        return Ok(());
    };
    let invalid = value.is_empty()
        || value.len() > 256
        || value.trim() != value
        || value.contains('\\')
        || value.chars().any(char::is_control)
        || (path && (value.starts_with('/') || value.split('/').any(|part| part == "..")));
    if invalid {
        Err(RepositoryError::InvalidInput)
    } else {
        Ok(())
    }
}

fn policy_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RetentionPolicyRecord> {
    let keep: i64 = row.get(1)?;
    let created: i64 = row.get(4)?;
    let updated: i64 = row.get(5)?;
    Ok(RetentionPolicyRecord {
        project_id: row.get(0)?,
        keep_latest: u32::try_from(keep).map_err(rows::conversion_error)?,
        path_glob: row.get(2)?,
        branch_glob: row.get(3)?,
        created_at_ms: u64::try_from(created).map_err(rows::conversion_error)?,
        updated_at_ms: u64::try_from(updated).map_err(rows::conversion_error)?,
    })
}

fn run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RetentionRunRecord> {
    let candidates: i64 = row.get(1)?;
    let deleted: i64 = row.get(2)?;
    let started: i64 = row.get(4)?;
    let completed: Option<i64> = row.get(5)?;
    Ok(RetentionRunRecord {
        id: row.get(0)?,
        candidate_count: u64::try_from(candidates).map_err(rows::conversion_error)?,
        deleted_count: u64::try_from(deleted).map_err(rows::conversion_error)?,
        status: row.get(3)?,
        started_at_ms: u64::try_from(started).map_err(rows::conversion_error)?,
        completed_at_ms: completed
            .map(|value| u64::try_from(value).map_err(rows::conversion_error))
            .transpose()?,
        error_summary: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::{policy_row, run_row};
    use rusqlite::Connection;

    #[test]
    fn retention_rows_reject_each_malformed_provider_field() {
        let connection = Connection::open_in_memory().expect("connection");
        for query in [
            "SELECT 1, 1, NULL, NULL, 1, 1",
            "SELECT 'project', 'bad', NULL, NULL, 1, 1",
            "SELECT 'project', 1, 2, NULL, 1, 1",
            "SELECT 'project', 1, NULL, 2, 1, 1",
            "SELECT 'project', 1, NULL, NULL, 'bad', 1",
            "SELECT 'project', 1, NULL, NULL, 1, 'bad'",
            "SELECT 'project', -1, NULL, NULL, 1, 1",
            "SELECT 'project', 1, NULL, NULL, -1, 1",
            "SELECT 'project', 1, NULL, NULL, 1, -1",
        ] {
            assert!(connection.query_row(query, [], policy_row).is_err());
        }
        for query in [
            "SELECT 1, 1, 0, 'running', 1, NULL, NULL",
            "SELECT 'run', 'bad', 0, 'running', 1, NULL, NULL",
            "SELECT 'run', 1, 'bad', 'running', 1, NULL, NULL",
            "SELECT 'run', 1, 0, 2, 1, NULL, NULL",
            "SELECT 'run', 1, 0, 'running', 'bad', NULL, NULL",
            "SELECT 'run', 1, 0, 'running', 1, 'bad', NULL",
            "SELECT 'run', 1, 0, 'running', 1, NULL, 2",
            "SELECT 'run', -1, 0, 'running', 1, NULL, NULL",
            "SELECT 'run', 1, -1, 'running', 1, NULL, NULL",
            "SELECT 'run', 1, 0, 'running', -1, NULL, NULL",
            "SELECT 'run', 1, 0, 'complete', 1, -1, NULL",
        ] {
            assert!(connection.query_row(query, [], run_row).is_err());
        }
    }
}
