use super::{lifecycle_audit, map_error, yard_history, yard_queries, yard_validation};
use blobyard_contract::{
    AuditValue, NewAuditEvent, NewYardFile, RepositoryError, YardDeployStatus, YardDeploymentRecord,
};
use rusqlite::{Transaction, params};

pub(super) fn finalise(
    transaction: &Transaction<'_>,
    deploy_id: &str,
    files: &[NewYardFile],
    finalised_at_ms: u64,
    event: &NewAuditEvent,
) -> Result<YardDeploymentRecord, RepositoryError> {
    let deploy = yard_queries::deploy_by_id(transaction, deploy_id)?;
    let yard = yard_queries::yard_by_id(transaction, &deploy.yard_id)?;
    if yard.status != blobyard_contract::WebYardStatus::Active {
        return Err(RepositoryError::Conflict);
    }
    if matches!(
        deploy.status,
        YardDeployStatus::Live | YardDeployStatus::Superseded
    ) {
        return graph(transaction, deploy);
    }
    let (finalised_at, file_count, total_bytes) =
        yard_validation::finalise(&deploy, files, finalised_at_ms)?;
    insert_files(transaction, &deploy, files)?;
    let status = promote_status(transaction, &deploy, finalised_at)?;
    let metadata_status = status.as_str().to_owned();
    yard_validation::action_event(
        event,
        "yard.deployed",
        "yard_deploy",
        &deploy.workspace_id,
        finalised_at_ms,
        [
            ("deployId", AuditValue::String(deploy.id.clone())),
            ("fileCount", AuditValue::Number(file_count.cast_unsigned())),
            ("status", AuditValue::String(metadata_status)),
            (
                "totalBytes",
                AuditValue::Number(total_bytes.cast_unsigned()),
            ),
        ],
    )?;
    transaction
        .execute(
            "UPDATE yard_deploys SET status = ?2, finalised_at_ms = ?3, file_count = ?4, total_bytes = ?5 WHERE id = ?1 AND status IN ('uploading', 'finalising')",
            params![deploy.id, status.as_str(), finalised_at, file_count, total_bytes],
        )
        .map_err(map_error)?;
    lifecycle_audit::insert(transaction, event)?;
    let yard = yard_queries::yard_by_id(transaction, &deploy.yard_id)?;
    yard_history::prune(
        transaction,
        &yard.id,
        yard.current_deploy_id.as_deref(),
        finalised_at,
    )?;
    graph(
        transaction,
        yard_queries::deploy_by_id(transaction, &deploy.id)?,
    )
}

fn insert_files(
    transaction: &Transaction<'_>,
    deploy: &blobyard_contract::YardDeployRecord,
    files: &[NewYardFile],
) -> Result<(), RepositoryError> {
    for file in files {
        let size = file.byte_size.cast_signed();
        let changed = transaction
            .execute(
                "INSERT INTO yard_deploy_files (deploy_id, normalized_path, version_id, byte_size) SELECT ?1, ?2, v.id, ?4 FROM object_versions v WHERE v.id = ?3 AND v.project_id = ?5 AND v.state = 'complete' AND v.size = ?4",
                params![deploy.id, file.normalized_path, file.version_id, size, deploy.project_id],
            )
            .map_err(map_error)?;
        if changed != 1 {
            return Err(RepositoryError::NotFound);
        }
    }
    Ok(())
}

fn promote_status(
    transaction: &Transaction<'_>,
    deploy: &blobyard_contract::YardDeployRecord,
    updated_at: i64,
) -> Result<YardDeployStatus, RepositoryError> {
    let newer: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM yard_deploys WHERE yard_id = ?1 AND finalised_at_ms IS NOT NULL AND (created_at_ms > ?2 OR (created_at_ms = ?2 AND id > ?3)))",
            params![deploy.yard_id, deploy.created_at_ms.cast_signed(), deploy.id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    if newer {
        return Ok(YardDeployStatus::Superseded);
    }
    transaction
        .execute(
            "UPDATE yard_deploys SET status = 'superseded' WHERE id = (SELECT current_deploy_id FROM web_yards WHERE id = ?1) AND id != ?2 AND status = 'live'",
            params![deploy.yard_id, deploy.id],
        )
        .map_err(map_error)?;
    let changed = transaction
        .execute(
            "UPDATE web_yards SET current_deploy_id = ?2, updated_at_ms = ?3 WHERE id = ?1 AND status = 'active'",
            params![deploy.yard_id, deploy.id, updated_at],
        )
        .map_err(map_error)?;
    if changed != 1 {
        return Err(RepositoryError::Conflict);
    }
    Ok(YardDeployStatus::Live)
}

fn graph(
    transaction: &Transaction<'_>,
    deploy: blobyard_contract::YardDeployRecord,
) -> Result<YardDeploymentRecord, RepositoryError> {
    let yard = yard_queries::yard_by_id(transaction, &deploy.yard_id)?;
    Ok(YardDeploymentRecord { yard, deploy })
}
