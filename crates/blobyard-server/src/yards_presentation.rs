use super::contracts::web_yard_url;
use crate::error::ApiError;
use blobyard_api_client::{
    StartYardDeployResponse, WebYardStatus as ApiYardStatus, WebYardSummary,
    YardDeployStatus as ApiDeployStatus, YardDeploySummary, YardDeploymentResponse,
};
use blobyard_contract::{
    WebYardRecord, WebYardStatus, YardDeployRecord, YardDeployStatus, YardDeploymentRecord,
    YardStartRecord,
};

pub(super) fn yard_summary(origin: &str, yard: WebYardRecord) -> Result<WebYardSummary, ApiError> {
    Ok(WebYardSummary {
        current_deploy_id: yard.current_deploy_id,
        url: web_yard_url(origin, &yard.host_label)?,
        host_label: yard.host_label,
        id: yard.id,
        name: yard.name,
        project_id: yard.project_id,
        status: yard_status(yard.status)?,
        workspace_id: yard.workspace_id,
    })
}

pub(super) fn deploy_summary(
    origin: &str,
    deploy: YardDeployRecord,
    current_deploy_id: Option<&str>,
) -> Result<YardDeploySummary, ApiError> {
    Ok(YardDeploySummary {
        clean_urls: deploy.clean_urls,
        client_deploy_id: deploy.client_deploy_id,
        created_at: deploy.created_at_ms,
        deployment_url: web_yard_url(origin, &deploy.deployment_host_label)?,
        file_count: deploy.file_count,
        finalised_at: deploy.finalised_at_ms,
        is_current: current_deploy_id == Some(deploy.id.as_str()),
        id: deploy.id,
        spa: deploy.spa,
        status: deploy_status(deploy.status),
        total_bytes: deploy.total_bytes,
    })
}

pub(super) fn start_response(
    origin: &str,
    record: YardStartRecord,
) -> Result<StartYardDeployResponse, ApiError> {
    Ok(StartYardDeployResponse {
        deploy_id: record.deploy.id,
        deployment_url: web_yard_url(origin, &record.deploy.deployment_host_label)?,
        host_label: record.yard.host_label.clone(),
        manifest_root: record.deploy.manifest_root,
        status: deploy_status(record.deploy.status),
        url: web_yard_url(origin, &record.yard.host_label)?,
        yard_id: record.yard.id,
        yard_name: record.yard.name,
    })
}

pub(super) fn deployment_response(
    origin: &str,
    record: YardDeploymentRecord,
) -> Result<YardDeploymentResponse, ApiError> {
    Ok(YardDeploymentResponse {
        deploy_id: record.deploy.id,
        deployment_url: web_yard_url(origin, &record.deploy.deployment_host_label)?,
        status: deploy_status(record.deploy.status),
        url: web_yard_url(origin, &record.yard.host_label)?,
    })
}

const fn yard_status(status: WebYardStatus) -> Result<ApiYardStatus, ApiError> {
    match status {
        WebYardStatus::Active => Ok(ApiYardStatus::Active),
        WebYardStatus::Suspended => Ok(ApiYardStatus::Suspended),
        WebYardStatus::Deleted => Err(ApiError::not_found()),
    }
}

pub(super) const fn deploy_status(status: YardDeployStatus) -> ApiDeployStatus {
    match status {
        YardDeployStatus::Uploading => ApiDeployStatus::Uploading,
        YardDeployStatus::Finalising => ApiDeployStatus::Finalising,
        YardDeployStatus::Live => ApiDeployStatus::Live,
        YardDeployStatus::Failed => ApiDeployStatus::Failed,
        YardDeployStatus::Superseded => ApiDeployStatus::Superseded,
        YardDeployStatus::Pruned => ApiDeployStatus::Pruned,
    }
}
