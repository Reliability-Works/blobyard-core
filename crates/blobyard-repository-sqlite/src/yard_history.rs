use super::{map_error, yard_cleanup, yard_queries, yard_validation};
use blobyard_contract::RepositoryError;
use rusqlite::{Transaction, params};

pub(super) fn prune(
    transaction: &Transaction<'_>,
    yard_id: &str,
    current_deploy_id: Option<&str>,
    pruned_at: i64,
) -> Result<(), RepositoryError> {
    let deploys = list_candidates(transaction, yard_id)?;
    let retained_others = if current_deploy_id.is_some() {
        yard_validation::HISTORY_DEPTH - 1
    } else {
        yard_validation::HISTORY_DEPTH
    };
    for deploy in deploys
        .into_iter()
        .filter(|deploy| Some(deploy.id.as_str()) != current_deploy_id)
        .skip(retained_others)
        .filter(|deploy| {
            matches!(
                deploy.status,
                blobyard_contract::YardDeployStatus::Live
                    | blobyard_contract::YardDeployStatus::Failed
                    | blobyard_contract::YardDeployStatus::Superseded
            )
        })
    {
        mark_pruned(transaction, &deploy.id, pruned_at)?;
    }
    Ok(())
}

pub(super) fn prune_all(
    transaction: &Transaction<'_>,
    yard_id: &str,
    pruned_at: i64,
) -> Result<(), RepositoryError> {
    let deploys = list_candidates(transaction, yard_id)?;
    for deploy in deploys {
        if deploy.status != blobyard_contract::YardDeployStatus::Pruned {
            mark_pruned(transaction, &deploy.id, pruned_at)?;
        }
    }
    Ok(())
}

fn list_candidates(
    transaction: &Transaction<'_>,
    yard_id: &str,
) -> Result<Vec<blobyard_contract::YardDeployRecord>, RepositoryError> {
    let mut statement = transaction
        .prepare(&format!(
            "SELECT {} FROM yard_deploys WHERE yard_id = ?1 ORDER BY created_at_ms DESC, id DESC",
            super::yard_rows::DEPLOY_COLUMNS
        ))
        .map_err(map_error)?;
    yard_queries::list_deploys(&mut statement, yard_id)
}

fn mark_pruned(
    transaction: &Transaction<'_>,
    deploy_id: &str,
    pruned_at: i64,
) -> Result<(), RepositoryError> {
    yard_cleanup::plan(transaction, deploy_id, pruned_at)?;
    transaction
        .execute(
            "DELETE FROM yard_deploy_files WHERE deploy_id = ?1",
            [deploy_id],
        )
        .map_err(map_error)?;
    transaction
        .execute(
            "UPDATE yard_deploys SET status = 'pruned', pruned_at_ms = ?2 WHERE id = ?1 AND status != 'pruned'",
            params![deploy_id, pruned_at],
        )
        .map(|_changed| ())
        .map_err(map_error)
}
