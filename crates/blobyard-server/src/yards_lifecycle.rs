use super::{presentation::deployment_response, read::authorize_yard};
use crate::{
    api::AppState,
    audit,
    auth::Principal,
    error::ApiError,
    response::{Success, success},
};
use axum::Json;
use blobyard_api_client::{
    DeleteWebYardRequest, EmptyResponse, RollbackWebYardRequest, YardDeploymentResponse,
};
use blobyard_contract::AuditValue;

pub(super) fn rollback(
    state: &AppState,
    principal: &Principal,
    request: &RollbackWebYardRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<YardDeploymentResponse>>, ApiError> {
    let yard = state
        .repository
        .web_yard_by_id(&request.yard_id)
        .map_err(ApiError::from_repository)?;
    authorize_yard(principal, &yard)?;
    let now = now?;
    let selected = selected_rollback_id(state, &yard, request.deploy_id.as_deref())?;
    let event = audit::event(
        yard.workspace_id.clone(),
        principal.0.id.clone(),
        "yard.rolled_back",
        "yard_deploy",
        vec![
            ("deployId".to_owned(), AuditValue::String(selected.clone())),
            ("yardId".to_owned(), AuditValue::String(yard.id.clone())),
        ],
        now,
    );
    let record = state
        .repository
        .rollback_web_yard(&yard.id, Some(&selected), now, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(deployment_response(
        &state.web_yard_origin,
        record,
    )?))
}

pub(super) fn delete(
    state: &AppState,
    principal: &Principal,
    request: &DeleteWebYardRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let yard = state
        .repository
        .web_yard_by_id(&request.yard_id)
        .map_err(ApiError::from_repository)?;
    authorize_yard(principal, &yard)?;
    let now = now?;
    let event = audit::event(
        yard.workspace_id.clone(),
        principal.0.id.clone(),
        "yard.deleted",
        "web_yard",
        vec![("yardId".to_owned(), AuditValue::String(yard.id.clone()))],
        now,
    );
    state
        .repository
        .delete_web_yard(&yard.id, now, &event)
        .map_err(ApiError::from_repository)?;
    crate::yard_cleanup::execute_for_yard(state, &yard.id, now)?;
    Ok(success(EmptyResponse::default()))
}

fn selected_rollback_id(
    state: &AppState,
    yard: &blobyard_contract::WebYardRecord,
    requested: Option<&str>,
) -> Result<String, ApiError> {
    state
        .repository
        .list_yard_deploys(&yard.id)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .find(|deploy| {
            deploy.status == blobyard_contract::YardDeployStatus::Superseded
                && deploy.finalised_at_ms.is_some()
                && Some(deploy.id.as_str()) != yard.current_deploy_id.as_deref()
                && requested.is_none_or(|id| id == deploy.id)
        })
        .map(|deploy| deploy.id)
        .ok_or_else(ApiError::not_found)
}
