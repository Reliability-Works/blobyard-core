use super::rows;
use blobyard_contract::{WebYardRecord, WebYardStatus, YardDeployRecord, YardDeployStatus};
use blobyard_core::Slug;
use rusqlite::Row;

pub(super) const YARD_COLUMNS: &str = "id, workspace_id, project_id, name, host_label, current_deploy_id, status, created_at_ms, updated_at_ms, deleted_at_ms";
pub(super) const DEPLOY_COLUMNS: &str = "id, yard_id, workspace_id, project_id, client_deploy_id, manifest_root, deployment_host_label, spa, clean_urls, status, created_at_ms, finalised_at_ms, file_count, total_bytes";
pub(super) const QUALIFIED_DEPLOY_COLUMNS: &str = "d.id, d.yard_id, d.workspace_id, d.project_id, d.client_deploy_id, d.manifest_root, d.deployment_host_label, d.spa, d.clean_urls, d.status, d.created_at_ms, d.finalised_at_ms, d.file_count, d.total_bytes";

pub(super) fn yard(row: &Row<'_>) -> rusqlite::Result<WebYardRecord> {
    let name: String = row.get(3)?;
    let status: String = row.get(6)?;
    Ok(WebYardRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        project_id: row.get(2)?,
        name: Slug::new(name.clone()).map_err(|_error| rows::conversion_error(name))?,
        host_label: row.get(4)?,
        current_deploy_id: row.get(5)?,
        status: WebYardStatus::parse(&status).ok_or_else(|| rows::conversion_error(status))?,
        created_at_ms: required_u64(row.get(7)?)?,
        updated_at_ms: required_u64(row.get(8)?)?,
        deleted_at_ms: optional_u64(row.get(9)?)?,
    })
}

pub(super) fn deploy(row: &Row<'_>) -> rusqlite::Result<YardDeployRecord> {
    let status: String = row.get(9)?;
    Ok(YardDeployRecord {
        id: row.get(0)?,
        yard_id: row.get(1)?,
        workspace_id: row.get(2)?,
        project_id: row.get(3)?,
        client_deploy_id: row.get(4)?,
        manifest_root: row.get(5)?,
        deployment_host_label: row.get(6)?,
        spa: row.get(7)?,
        clean_urls: row.get(8)?,
        status: YardDeployStatus::parse(&status).ok_or_else(|| rows::conversion_error(status))?,
        created_at_ms: required_u64(row.get(10)?)?,
        finalised_at_ms: optional_u64(row.get(11)?)?,
        file_count: required_u64(row.get(12)?)?,
        total_bytes: required_u64(row.get(13)?)?,
    })
}

fn required_u64(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(rows::conversion_error)
}

fn optional_u64(value: Option<i64>) -> rusqlite::Result<Option<u64>> {
    value
        .map(|number| u64::try_from(number).map_err(rows::conversion_error))
        .transpose()
}

#[cfg(test)]
#[path = "yard_rows_tests.rs"]
mod tests;
