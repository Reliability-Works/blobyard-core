use super::{lifecycle_deletion, map_error, yard_queries};
use blobyard_contract::{RepositoryError, YardCleanupPlan};
use rusqlite::{Connection, Statement, Transaction, params};

const ACTOR: &str = "system:yard-cleanup";

pub(super) fn plan(
    transaction: &Transaction<'_>,
    deploy_id: &str,
    created_at_ms: i64,
) -> Result<(), RepositoryError> {
    let deploy = yard_queries::deploy_by_id(transaction, deploy_id)?;
    let operation_id = format!("yardcleanup_{}", deploy.id);
    let request_id = format!("request_yardcleanup_{}", deploy.id);
    let inserted = transaction
        .execute(
            "INSERT OR IGNORE INTO deletion_operations (id, project_id, object_path, selected_version, reason, status, actor, request_id, created_at_ms) SELECT ?1, ?2, ?3, NULL, 'yard_cleanup', 'pending', ?4, ?5, ?6 WHERE EXISTS (SELECT 1 FROM yard_deploy_files f JOIN object_versions v ON v.id = f.version_id WHERE f.deploy_id = ?7 AND (v.object_path = ?3 OR substr(v.object_path, 1, length(?3)) = ?3))",
            params![
                operation_id,
                deploy.project_id,
                deploy.manifest_root,
                ACTOR,
                request_id,
                created_at_ms,
                deploy.id,
            ],
        )
        .map_err(map_error)?;
    if inserted == 0 {
        return Ok(());
    }
    transaction
        .execute(
            "INSERT INTO deletion_items (operation_id, version_id, storage_key, version) SELECT ?1, v.id, v.storage_key, v.version FROM yard_deploy_files f JOIN object_versions v ON v.id = f.version_id WHERE f.deploy_id = ?2 AND v.state = 'complete' AND (v.object_path = ?3 OR substr(v.object_path, 1, length(?3)) = ?3) AND NOT EXISTS (SELECT 1 FROM yard_deploy_files other JOIN yard_deploys d ON d.id = other.deploy_id WHERE other.version_id = v.id AND other.deploy_id != ?2 AND d.status != 'pruned') ORDER BY v.id",
            params![operation_id, deploy.id, deploy.manifest_root],
        )
        .map(|_inserted| ())
        .map_err(map_error)
}

pub(super) fn pending(
    connection: &Connection,
    yard_id: Option<&str>,
) -> Result<Vec<YardCleanupPlan>, RepositoryError> {
    let mut statement = connection
        .prepare(
            "SELECT o.id, d.yard_id, d.workspace_id, d.id FROM deletion_operations o JOIN yard_deploys d ON d.project_id = o.project_id AND d.manifest_root = o.object_path WHERE o.reason = 'yard_cleanup' AND o.status = 'pending' AND (?1 IS NULL OR d.yard_id = ?1) ORDER BY o.created_at_ms, o.id",
        )
        .map_err(map_error)?;
    let headers = query_headers(&mut statement, yard_id)?;
    headers
        .into_iter()
        .map(|(operation_id, yard_id, workspace_id, deploy_id)| {
            Ok(YardCleanupPlan {
                yard_id,
                workspace_id,
                deploy_id,
                deletion: lifecycle_deletion::deletion_plan(connection, &operation_id)?,
            })
        })
        .collect()
}

pub(super) fn query_headers(
    statement: &mut Statement<'_>,
    yard_id: Option<&str>,
) -> Result<Vec<(String, String, String, String)>, RepositoryError> {
    statement
        .query_map([yard_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(map_error)?
        .collect::<Result<Vec<(String, String, String, String)>, _>>()
        .map_err(map_error)
}

#[cfg(test)]
#[path = "yard_cleanup_tests.rs"]
mod tests;
