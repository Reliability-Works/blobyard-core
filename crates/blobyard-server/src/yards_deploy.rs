use super::{
    contracts::{
        deployment_host_label, manifest_root, snapshot_manifest, stable_host_label,
        validate_yard_name,
    },
    presentation::{deployment_response, start_response},
    read::yard_for_deploy,
};
use crate::{
    api::AppState,
    audit,
    auth::Principal,
    error::ApiError,
    response::{Success, success},
    transfer_grants as grants,
};
use axum::Json;
use blobyard_api_client::{
    EmptyResponse, FailYardDeployRequest, StartYardDeployRequest, StartYardDeployResponse,
    YardDeployMutationRequest, YardDeploymentResponse,
};
use blobyard_contract::{
    AuditValue, NewWebYard, NewYardDeploy, WebYardStatus, YardDeployRecord, YardDeployStatus,
};

pub(super) fn start(
    state: &AppState,
    principal: &Principal,
    request: &StartYardDeployRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<StartYardDeployResponse>>, ApiError> {
    if !request.public {
        return Err(ApiError::invalid_request());
    }
    validate_yard_name(&request.name)?;
    let project = grants::resolve_authorized_project(
        state,
        &principal.0,
        &request.workspace,
        &request.project,
    )?;
    let now = now?;
    let yard_id = format!("yard_{}", uuid::Uuid::new_v4().simple());
    let deploy_id = format!("yarddeploy_{}", uuid::Uuid::new_v4().simple());
    let yard = NewWebYard {
        id: yard_id.clone(),
        workspace_id: principal.0.workspace_id.clone(),
        project_id: project.id.clone(),
        name: request.name.clone(),
        host_label: stable_host_label(&request.name, &request.workspace, &yard_id),
        created_at_ms: now,
    };
    let deploy = NewYardDeploy {
        id: deploy_id.clone(),
        yard_id: yard_id.clone(),
        workspace_id: principal.0.workspace_id.clone(),
        project_id: project.id,
        client_deploy_id: request.client_deploy_id.clone(),
        manifest_root: manifest_root(&yard_id, &request.client_deploy_id),
        deployment_host_label: deployment_host_label(&request.name, &request.workspace, &deploy_id),
        spa: request.spa,
        clean_urls: request.clean_urls,
        created_at_ms: now,
    };
    let event = audit::event(
        principal.0.workspace_id.clone(),
        principal.0.id.clone(),
        "yard.created",
        "web_yard",
        vec![("yardId".to_owned(), AuditValue::String(yard_id))],
        now,
    );
    let record = state
        .repository
        .start_yard_deploy(&yard, &deploy, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(start_response(&state.web_yard_origin, record)?))
}

pub(super) fn finalise(
    state: &AppState,
    principal: &Principal,
    request: &YardDeployMutationRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<YardDeploymentResponse>>, ApiError> {
    let deploy = state
        .repository
        .yard_deploy_by_id(&request.deploy_id)
        .map_err(ApiError::from_repository)?;
    let yard = yard_for_deploy(state, principal, &deploy)?;
    if yard.status != WebYardStatus::Active {
        return Err(ApiError::conflict());
    }
    let objects = state
        .repository
        .list_stored_objects(&deploy.project_id, Some(&deploy.manifest_root), false)
        .map_err(ApiError::from_repository)?;
    let snapshot = snapshot_manifest(&deploy.manifest_root, objects)?;
    let now = now?;
    let file_count = snapshot.files.len() as u64;
    let status = predicted_status(state, &deploy)?;
    let event = audit::event(
        deploy.workspace_id.clone(),
        principal.0.id.clone(),
        "yard.deployed",
        "yard_deploy",
        vec![
            ("deployId".to_owned(), AuditValue::String(deploy.id.clone())),
            ("fileCount".to_owned(), AuditValue::Number(file_count)),
            (
                "status".to_owned(),
                AuditValue::String(status.as_str().to_owned()),
            ),
            (
                "totalBytes".to_owned(),
                AuditValue::Number(snapshot.total_bytes),
            ),
        ],
        now,
    );
    let record = state
        .repository
        .finalise_yard_deploy(&deploy.id, &snapshot.files, now, &event)
        .map_err(ApiError::from_repository)?;
    crate::yard_cleanup::execute_for_yard(state, &yard.id, now)?;
    Ok(success(deployment_response(
        &state.web_yard_origin,
        record,
    )?))
}

pub(super) fn fail(
    state: &AppState,
    principal: &Principal,
    request: &FailYardDeployRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let deploy = state
        .repository
        .yard_deploy_by_id(&request.deploy_id)
        .map_err(ApiError::from_repository)?;
    yard_for_deploy(state, principal, &deploy)?;
    let failed_at_ms = now?;
    let failed = state
        .repository
        .fail_yard_deploy(
            &deploy.id,
            &request.failure_code,
            &request.failure_message,
            failed_at_ms,
        )
        .map_err(ApiError::from_repository)?;
    crate::yard_cleanup::execute_for_yard(state, &failed.yard_id, failed_at_ms)?;
    Ok(success(EmptyResponse::default()))
}

fn predicted_status(
    state: &AppState,
    deploy: &YardDeployRecord,
) -> Result<YardDeployStatus, ApiError> {
    let newer = state
        .repository
        .list_yard_deploys(&deploy.yard_id)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .any(|candidate| {
            candidate.finalised_at_ms.is_some()
                && (candidate.created_at_ms > deploy.created_at_ms
                    || (candidate.created_at_ms == deploy.created_at_ms
                        && candidate.id > deploy.id))
        });
    Ok(if newer {
        YardDeployStatus::Superseded
    } else {
        YardDeployStatus::Live
    })
}
