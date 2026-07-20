use super::{lifecycle_audit, lifecycle_operation, map_error, rows};
use blobyard_contract::{
    DeletionItem, DeletionPlan, NewAuditEvent, NewObjectDeletion, RepositoryError,
};
use rusqlite::{Connection, OptionalExtension, Statement, Transaction, params};

#[derive(Clone, Copy)]
pub(super) struct ValidatedDeletion {
    selected_version: Option<i64>,
    created_at_ms: i64,
}

pub(super) fn validate(value: &NewObjectDeletion) -> Result<ValidatedDeletion, RepositoryError> {
    for text in [
        &value.id,
        &value.target.project_id,
        &value.target.object_path,
        &value.actor,
        &value.request_id,
    ] {
        rows::validate_text(text)?;
    }
    if value.target.version == Some(0) {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(ValidatedDeletion {
        selected_version: value
            .target
            .version
            .map(lifecycle_audit::to_i64)
            .transpose()?,
        created_at_ms: lifecycle_audit::to_i64(value.created_at_ms)?,
    })
}

pub(super) fn begin(
    transaction: &Transaction<'_>,
    value: &NewObjectDeletion,
    validated: ValidatedDeletion,
) -> Result<DeletionPlan, RepositoryError> {
    let versions = selected_versions(transaction, value, validated.selected_version)?;
    if let Some(id) = pending_operation(transaction, value, validated.selected_version)? {
        return deletion_plan(transaction, &id);
    }
    if versions.is_empty() {
        return completed_operation(transaction, value, validated.selected_version)?
            .map(|id| deletion_plan(transaction, &id))
            .transpose()?
            .ok_or(RepositoryError::NotFound);
    }
    if versions.iter().any(|version| version.state == "pending") {
        return Err(RepositoryError::Conflict);
    }
    transaction
        .execute(
            "INSERT INTO deletion_operations (id, project_id, object_path, selected_version, reason, status, actor, request_id, created_at_ms) VALUES (?1, ?2, ?3, ?4, 'object_delete', 'pending', ?5, ?6, ?7)",
            params![
                value.id,
                value.target.project_id,
                value.target.object_path,
                validated.selected_version,
                value.actor,
                value.request_id,
                validated.created_at_ms,
            ],
        )
        .map_err(map_error)?;
    insert_complete_items(transaction, &value.id, &versions)?;
    deletion_plan(transaction, &value.id)
}

pub(super) fn deletion_plan(
    connection: &Connection,
    operation_id: &str,
) -> Result<DeletionPlan, RepositoryError> {
    let (complete, actor, request_id) = connection
        .query_row(
            "SELECT status = 'complete', actor, request_id FROM deletion_operations WHERE id = ?1",
            [operation_id],
            deletion_header_row,
        )
        .map_err(map_error)?;
    let mut statement = connection
        .prepare(
            "SELECT version_id, storage_key, version FROM deletion_items WHERE operation_id = ?1 ORDER BY version_id",
        )
        .map_err(map_error)?;
    let items = query_deletion_items(&mut statement, operation_id)?;
    Ok(DeletionPlan {
        id: operation_id.to_owned(),
        items,
        complete,
        actor,
        request_id,
    })
}

fn query_deletion_items(
    statement: &mut Statement<'_>,
    operation_id: &str,
) -> Result<Vec<DeletionItem>, RepositoryError> {
    statement
        .query_map([operation_id], item_row)
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)
}

fn deletion_header_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(bool, String, String)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
}

pub(super) fn finish(
    transaction: &Transaction<'_>,
    operation_id: &str,
    completed_at_ms: i64,
    event: &NewAuditEvent,
) -> Result<(), RepositoryError> {
    let operation = lifecycle_operation::operation(transaction, operation_id)?;
    if operation.complete {
        return Ok(());
    }
    lifecycle_operation::validate_event(transaction, &operation, event)?;
    delete_selected_metadata(transaction, &operation)?;
    transaction
        .execute(
            "UPDATE deletion_operations SET status = 'complete', completed_at_ms = ?2 WHERE id = ?1 AND status = 'pending'",
            params![operation_id, completed_at_ms],
        )
        .map_err(map_error)?;
    lifecycle_operation::complete_run(transaction, &operation, completed_at_ms)?;
    lifecycle_audit::insert(transaction, event)
}

struct SelectedVersion {
    id: String,
    storage_key: String,
    version: i64,
    state: String,
}

fn selected_versions(
    transaction: &Transaction<'_>,
    value: &NewObjectDeletion,
    selected: Option<i64>,
) -> Result<Vec<SelectedVersion>, RepositoryError> {
    let mut statement = transaction
        .prepare(
            "SELECT id, storage_key, version, state FROM object_versions WHERE project_id = ?1 AND object_path = ?2 AND (?3 IS NULL OR version = ?3) ORDER BY version",
        )
        .map_err(map_error)?;
    query_selected_versions(
        &mut statement,
        &value.target.project_id,
        &value.target.object_path,
        selected,
    )
}

fn query_selected_versions(
    statement: &mut Statement<'_>,
    project_id: &str,
    object_path: &str,
    selected: Option<i64>,
) -> Result<Vec<SelectedVersion>, RepositoryError> {
    statement
        .query_map(params![project_id, object_path, selected], selected_row)
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)
}

fn selected_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SelectedVersion> {
    let version: i64 = row.get(2)?;
    u64::try_from(version).map_err(rows::conversion_error)?;
    Ok(SelectedVersion {
        id: row.get(0)?,
        storage_key: row.get(1)?,
        version,
        state: row.get(3)?,
    })
}

fn pending_operation(
    transaction: &Transaction<'_>,
    value: &NewObjectDeletion,
    selected: Option<i64>,
) -> Result<Option<String>, RepositoryError> {
    target_operation(transaction, value, selected, false)
}

fn completed_operation(
    transaction: &Transaction<'_>,
    value: &NewObjectDeletion,
    selected: Option<i64>,
) -> Result<Option<String>, RepositoryError> {
    target_operation(transaction, value, selected, true)
}

fn target_operation(
    transaction: &Transaction<'_>,
    value: &NewObjectDeletion,
    selected: Option<i64>,
    complete: bool,
) -> Result<Option<String>, RepositoryError> {
    let query = if complete {
        "SELECT id FROM deletion_operations WHERE project_id = ?1 AND object_path = ?2 AND reason = 'object_delete' AND status = 'complete' AND ((selected_version IS NULL AND ?3 IS NULL) OR selected_version = ?3) ORDER BY completed_at_ms DESC, id DESC LIMIT 1"
    } else {
        "SELECT id FROM deletion_operations WHERE project_id = ?1 AND object_path = ?2 AND reason = 'object_delete' AND status = 'pending' AND ((selected_version IS NULL AND ?3 IS NULL) OR selected_version = ?3)"
    };
    transaction
        .query_row(
            query,
            params![value.target.project_id, value.target.object_path, selected],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_error)
}

fn insert_complete_items(
    transaction: &Transaction<'_>,
    operation_id: &str,
    versions: &[SelectedVersion],
) -> Result<(), RepositoryError> {
    for version in versions
        .iter()
        .filter(|version| version.state == "complete")
    {
        transaction
            .execute(
                "INSERT INTO deletion_items (operation_id, version_id, storage_key, version) VALUES (?1, ?2, ?3, ?4)",
                params![
                    operation_id,
                    version.id,
                    version.storage_key,
                    version.version,
                ],
            )
            .map_err(map_error)?;
    }
    Ok(())
}

fn item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DeletionItem> {
    let version: i64 = row.get(2)?;
    Ok(DeletionItem {
        version_id: row.get(0)?,
        storage_key: row.get(1)?,
        version: u64::try_from(version).map_err(rows::conversion_error)?,
    })
}

fn delete_selected_metadata(
    transaction: &Transaction<'_>,
    operation: &lifecycle_operation::Operation,
) -> Result<(), RepositoryError> {
    for id in lifecycle_operation::operation_version_ids(transaction, &operation.id)? {
        transaction
            .execute("DELETE FROM download_grants WHERE version_id = ?1", [&id])
            .map_err(map_error)?;
        transaction
            .execute(
                "DELETE FROM upload_reservations WHERE version_id = ?1",
                [&id],
            )
            .map_err(map_error)?;
        transaction
            .execute("DELETE FROM object_versions WHERE id = ?1", [&id])
            .map_err(map_error)?;
    }
    if operation.reason == "object_delete" {
        lifecycle_operation::delete_aborted_target(transaction, operation)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "lifecycle_deletion_tests.rs"]
mod tests;
