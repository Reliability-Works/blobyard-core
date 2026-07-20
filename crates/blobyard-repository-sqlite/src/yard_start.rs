use super::{lifecycle_audit, map_error, yard_queries, yard_validation};
use blobyard_contract::{
    NewAuditEvent, NewWebYard, NewYardDeploy, RepositoryError, WebYardStatus, YardStartRecord,
};
use rusqlite::{Transaction, params};

pub(super) fn start(
    transaction: &Transaction<'_>,
    yard: &NewWebYard,
    deploy: &NewYardDeploy,
    event: &NewAuditEvent,
) -> Result<YardStartRecord, RepositoryError> {
    let (yard_created, deploy_created) = yard_validation::start(yard, deploy, event)?;
    if let Some(existing) =
        yard_queries::deploy_by_client_id(transaction, &deploy.client_deploy_id)?
    {
        return reused(transaction, yard, deploy, existing);
    }
    let selected = if let Some(existing) =
        yard_queries::active_named_yard(transaction, &yard.project_id, yard.name.as_str())?
    {
        if existing.status != WebYardStatus::Active || existing.workspace_id != yard.workspace_id {
            return Err(RepositoryError::Conflict);
        }
        existing
    } else {
        insert_yard(transaction, yard, yard_created)?;
        lifecycle_audit::insert(transaction, event)?;
        yard_queries::yard_by_id(transaction, &yard.id)?
    };
    insert_deploy(transaction, &selected, deploy, deploy_created)?;
    Ok(YardStartRecord {
        deploy: yard_queries::deploy_by_id(transaction, &deploy.id)?,
        yard: selected,
    })
}

fn reused(
    transaction: &Transaction<'_>,
    requested_yard: &NewWebYard,
    requested_deploy: &NewYardDeploy,
    existing: blobyard_contract::YardDeployRecord,
) -> Result<YardStartRecord, RepositoryError> {
    let yard = yard_queries::yard_by_id(transaction, &existing.yard_id)?;
    let matches = yard.status == WebYardStatus::Active
        && yard.workspace_id == requested_yard.workspace_id
        && yard.project_id == requested_yard.project_id
        && yard.name == requested_yard.name
        && existing.workspace_id == requested_deploy.workspace_id
        && existing.project_id == requested_deploy.project_id
        && existing.spa == requested_deploy.spa
        && existing.clean_urls == requested_deploy.clean_urls;
    if matches {
        Ok(YardStartRecord {
            yard,
            deploy: existing,
        })
    } else {
        Err(RepositoryError::Conflict)
    }
}

fn insert_yard(
    transaction: &Transaction<'_>,
    yard: &NewWebYard,
    created_at: i64,
) -> Result<(), RepositoryError> {
    let changed = transaction
        .execute(
            "INSERT INTO web_yards (id, workspace_id, project_id, name, host_label, current_deploy_id, status, created_at_ms, updated_at_ms, deleted_at_ms) SELECT ?1, ?2, p.id, ?4, ?5, NULL, 'active', ?6, ?6, NULL FROM projects p WHERE p.id = ?3 AND p.workspace_id = ?2",
            params![
                yard.id,
                yard.workspace_id,
                yard.project_id,
                yard.name.as_str(),
                yard.host_label,
                created_at,
            ],
        )
        .map_err(map_error)?;
    if changed == 1 {
        Ok(())
    } else {
        Err(RepositoryError::NotFound)
    }
}

fn insert_deploy(
    transaction: &Transaction<'_>,
    yard: &blobyard_contract::WebYardRecord,
    deploy: &NewYardDeploy,
    created_at: i64,
) -> Result<(), RepositoryError> {
    let root = format!(".blobyard-yard/{}/{}/", yard.id, deploy.client_deploy_id);
    transaction
        .execute(
            "INSERT INTO yard_deploys (id, yard_id, workspace_id, project_id, client_deploy_id, manifest_root, deployment_host_label, spa, clean_urls, status, created_at_ms, finalised_at_ms, file_count, total_bytes, failure_code, failure_message, pruned_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'uploading', ?10, NULL, 0, 0, NULL, NULL, NULL)",
            params![
                deploy.id,
                yard.id,
                yard.workspace_id,
                yard.project_id,
                deploy.client_deploy_id,
                root,
                deploy.deployment_host_label,
                deploy.spa,
                deploy.clean_urls,
                created_at,
            ],
        )
        .map(|_changed| ())
        .map_err(map_error)
}
