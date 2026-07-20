use super::{lifecycle_audit, map_error, yard_history, yard_queries, yard_validation};
use blobyard_contract::{
    AuditValue, NewAuditEvent, RepositoryError, WebYardStatus, YardDeployRecord, YardDeployStatus,
    YardDeploymentRecord,
};
use rusqlite::{Transaction, params};

pub(super) fn fail(
    transaction: &Transaction<'_>,
    deploy_id: &str,
    code: &str,
    message: &str,
    failed_at_ms: u64,
) -> Result<YardDeployRecord, RepositoryError> {
    let deploy = yard_queries::deploy_by_id(transaction, deploy_id)?;
    if matches!(
        deploy.status,
        YardDeployStatus::Failed | YardDeployStatus::Pruned
    ) {
        return Ok(deploy);
    }
    let failed_at = yard_validation::failure(&deploy, code, message, failed_at_ms)?;
    transaction
        .execute(
            "UPDATE yard_deploys SET status = 'failed', failure_code = ?2, failure_message = ?3 WHERE id = ?1 AND status IN ('uploading', 'finalising')",
            params![deploy.id, code, message],
        )
        .map_err(map_error)?;
    let yard = yard_queries::yard_by_id(transaction, &deploy.yard_id)?;
    yard_history::prune(
        transaction,
        &yard.id,
        yard.current_deploy_id.as_deref(),
        failed_at,
    )?;
    yard_queries::deploy_by_id(transaction, &deploy.id)
}

pub(super) fn rollback(
    transaction: &Transaction<'_>,
    yard_id: &str,
    deploy_id: Option<&str>,
    rolled_back_at_ms: u64,
    event: &NewAuditEvent,
) -> Result<YardDeploymentRecord, RepositoryError> {
    let yard = yard_queries::yard_by_id(transaction, yard_id)?;
    if yard.status != WebYardStatus::Active {
        return Err(RepositoryError::NotFound);
    }
    let target = rollback_target(transaction, &yard, deploy_id)?;
    let rolled_back_at = yard_validation::action_event(
        event,
        "yard.rolled_back",
        "yard_deploy",
        &yard.workspace_id,
        rolled_back_at_ms,
        [
            ("deployId", AuditValue::String(target.id.clone())),
            ("yardId", AuditValue::String(yard.id.clone())),
        ],
    )?;
    transaction
        .execute(
            "UPDATE yard_deploys SET status = 'superseded' WHERE id = ?1 AND status = 'live'",
            [yard.current_deploy_id.as_deref()],
        )
        .map_err(map_error)?;
    transaction
        .execute(
            "UPDATE yard_deploys SET status = 'live' WHERE id = ?1 AND status = 'superseded'",
            [&target.id],
        )
        .map_err(map_error)?;
    transaction
        .execute(
            "UPDATE web_yards SET current_deploy_id = ?2, updated_at_ms = ?3 WHERE id = ?1 AND status = 'active'",
            params![yard.id, target.id, rolled_back_at],
        )
        .map_err(map_error)?;
    lifecycle_audit::insert(transaction, event)?;
    Ok(YardDeploymentRecord {
        yard: yard_queries::yard_by_id(transaction, &yard.id)?,
        deploy: yard_queries::deploy_by_id(transaction, &target.id)?,
    })
}

pub(super) fn delete(
    transaction: &Transaction<'_>,
    yard_id: &str,
    deleted_at_ms: u64,
    event: &NewAuditEvent,
) -> Result<bool, RepositoryError> {
    let yard = yard_queries::yard_by_id(transaction, yard_id)?;
    if yard.status == WebYardStatus::Deleted {
        return Ok(false);
    }
    let deleted_at = yard_validation::action_event(
        event,
        "yard.deleted",
        "web_yard",
        &yard.workspace_id,
        deleted_at_ms,
        [("yardId", AuditValue::String(yard.id.clone()))],
    )?;
    yard_history::prune_all(transaction, &yard.id, deleted_at)?;
    transaction
        .execute(
            "UPDATE web_yards SET current_deploy_id = NULL, status = 'deleted', updated_at_ms = ?2, deleted_at_ms = ?2 WHERE id = ?1 AND status != 'deleted'",
            params![yard.id, deleted_at],
        )
        .map_err(map_error)?;
    lifecycle_audit::insert(transaction, event)?;
    Ok(true)
}

fn rollback_target(
    transaction: &Transaction<'_>,
    yard: &blobyard_contract::WebYardRecord,
    selected: Option<&str>,
) -> Result<YardDeployRecord, RepositoryError> {
    let mut statement = transaction
        .prepare(&format!(
            "SELECT {} FROM yard_deploys WHERE yard_id = ?1 ORDER BY created_at_ms DESC, id DESC",
            super::yard_rows::DEPLOY_COLUMNS
        ))
        .map_err(map_error)?;
    let deploys = yard_queries::list_deploys(&mut statement, &yard.id)?;
    deploys
        .into_iter()
        .find(|deploy| {
            deploy.status == YardDeployStatus::Superseded
                && deploy.finalised_at_ms.is_some()
                && Some(deploy.id.as_str()) != yard.current_deploy_id.as_deref()
                && selected.is_none_or(|id| id == deploy.id)
        })
        .ok_or(RepositoryError::NotFound)
}
