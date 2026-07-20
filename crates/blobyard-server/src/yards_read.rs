use super::presentation::{deploy_summary, yard_summary};
use crate::{
    api::AppState,
    auth::Principal,
    error::ApiError,
    response::{Page, Success, page, success},
    transfer_grants as grants,
};
use axum::Json;
use blobyard_api_client::{
    ListWebYardsQuery, ListYardDeploysQuery, WebYardSummary, YardDeploySummary,
};
use blobyard_contract::{WebYardRecord, YardDeployRecord};

pub(super) fn list(
    state: &AppState,
    principal: &Principal,
    query: &ListWebYardsQuery,
) -> Result<Json<Success<Page<WebYardSummary>>>, ApiError> {
    let project =
        grants::resolve_authorized_project(state, &principal.0, &query.workspace, &query.project)?;
    let items = state
        .repository
        .list_web_yards(&project.id)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .map(|yard| yard_summary(&state.web_yard_origin, yard))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(success(page(items)))
}

pub(super) fn list_deploys(
    state: &AppState,
    principal: &Principal,
    query: &ListYardDeploysQuery,
) -> Result<Json<Success<Page<YardDeploySummary>>>, ApiError> {
    let yard = state
        .repository
        .web_yard_by_id(&query.yard_id)
        .map_err(ApiError::from_repository)?;
    authorize_yard(principal, &yard)?;
    let deploys = state
        .repository
        .list_yard_deploys(&yard.id)
        .map_err(ApiError::from_repository)?;
    let ordered = current_then_history(deploys, yard.current_deploy_id.as_deref());
    let items = ordered
        .into_iter()
        .map(|deploy| {
            deploy_summary(
                &state.web_yard_origin,
                deploy,
                yard.current_deploy_id.as_deref(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(success(page(items)))
}

pub(super) fn authorize_yard(principal: &Principal, yard: &WebYardRecord) -> Result<(), ApiError> {
    let workspace_matches = yard.workspace_id == principal.0.workspace_id;
    let project_matches = principal
        .0
        .project_id
        .as_ref()
        .is_none_or(|project_id| project_id == &yard.project_id);
    if workspace_matches && project_matches {
        Ok(())
    } else {
        Err(ApiError::not_found())
    }
}

pub(super) fn yard_for_deploy(
    state: &AppState,
    principal: &Principal,
    deploy: &YardDeployRecord,
) -> Result<WebYardRecord, ApiError> {
    let yard = state
        .repository
        .web_yard_by_id(&deploy.yard_id)
        .map_err(ApiError::from_repository)?;
    authorize_yard(principal, &yard)?;
    if deploy.workspace_id == yard.workspace_id && deploy.project_id == yard.project_id {
        Ok(yard)
    } else {
        Err(ApiError::not_found())
    }
}

fn current_then_history(
    deploys: Vec<YardDeployRecord>,
    current_deploy_id: Option<&str>,
) -> Vec<YardDeployRecord> {
    let current = deploys
        .iter()
        .find(|deploy| Some(deploy.id.as_str()) == current_deploy_id)
        .cloned();
    let others = deploys
        .into_iter()
        .filter(|deploy| Some(deploy.id.as_str()) != current_deploy_id)
        .take(9);
    current.into_iter().chain(others).collect()
}
