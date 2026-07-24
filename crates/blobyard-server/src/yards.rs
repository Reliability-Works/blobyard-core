use crate::{api::AppState, auth::Principal, error::ApiError, response::Success};
use axum::{
    Json, Router,
    extract::{
        Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    routing::{get, post},
};
use blobyard_api_client::{
    DeleteWebYardRequest, EmptyResponse, FailYardDeployRequest, GetYardAccessQuery,
    GrantYardAccessRequest, ListWebYardsQuery, ListYardDeploysQuery, ListYardEnvironmentsQuery,
    RevokeYardAccessRequest, RollbackWebYardRequest, SetYardVisibilityRequest,
    StartYardDeployRequest, StartYardDeployResponse, WebYardSummary, YardAccessGrantResponse,
    YardAccessResponse, YardDeployMutationRequest, YardDeploySummary, YardDeploymentResponse,
    YardEnvironmentList, YardVisibilityResponse,
};
use blobyard_contract::CiAction;

#[path = "yards_access.rs"]
mod access;
#[path = "yards_contracts.rs"]
mod contracts;
#[path = "yards_deploy.rs"]
mod deploy;
#[path = "yards_lifecycle.rs"]
mod lifecycle;
#[path = "yards_presentation.rs"]
mod presentation;
#[path = "yards_public.rs"]
mod public;
#[path = "yards_read.rs"]
mod read;
#[path = "yards_sessions.rs"]
mod sessions;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/yards", get(list_yards))
        .route("/v1/yards/access", get(get_yard_access))
        .route("/v1/yards/access/grant", post(grant_yard_access))
        .route("/v1/yards/access/revoke", post(revoke_yard_access))
        .route("/v1/yards/access/visibility", post(set_yard_visibility))
        .route("/v1/yards/delete", post(delete_yard))
        .route("/v1/yards/deploys", get(list_yard_deploys))
        .route("/v1/yards/deploys/fail", post(fail_yard_deploy))
        .route("/v1/yards/deploys/finalise", post(finalise_yard_deploy))
        .route("/v1/yards/deploys/start", post(start_yard_deploy))
        .route("/v1/yards/environments", get(list_yard_environments))
        .route("/v1/yards/sessions", get(list_yard_sessions))
        .route("/v1/yards/sessions/revoke", post(revoke_yard_session))
        .route("/v1/yards/rollback", post(rollback_yard))
}

pub(crate) async fn public_fallback(
    state: axum::extract::State<AppState>,
    uri: axum::extract::OriginalUri,
    method: axum::http::Method,
    headers: axum::http::HeaderMap,
) -> Result<axum::http::Response<axum::body::Body>, ApiError> {
    public::public_fallback(state, uri, method, headers).await
}

async fn list_yards(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<ListWebYardsQuery>, QueryRejection>,
) -> Result<Json<Success<crate::response::Page<WebYardSummary>>>, ApiError> {
    require_read(&principal)?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    read::list(&state, &principal, &query)
}

async fn list_yard_deploys(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<ListYardDeploysQuery>, QueryRejection>,
) -> Result<Json<Success<crate::response::Page<YardDeploySummary>>>, ApiError> {
    require_read(&principal)?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    read::list_deploys(&state, &principal, &query)
}

async fn list_yard_environments(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<ListYardEnvironmentsQuery>, QueryRejection>,
) -> Result<Json<Success<YardEnvironmentList>>, ApiError> {
    require_read(&principal)?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    read::list_environments(&state, &principal, &query)
}

async fn get_yard_access(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<GetYardAccessQuery>, QueryRejection>,
) -> Result<Json<Success<YardAccessResponse>>, ApiError> {
    require_read(&principal)?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    access::get(&state, &principal, &query, crate::transfer_grants::now_ms())
}

async fn list_yard_sessions(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<blobyard_api_client::ListYardSessionsQuery>, QueryRejection>,
) -> Result<Json<Success<blobyard_api_client::ListYardSessionsResponse>>, ApiError> {
    require_read(&principal)?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    sessions::list(&state, &principal, &query, crate::transfer_grants::now_ms())
}

async fn revoke_yard_session(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<blobyard_api_client::RevokeYardSessionRequest>, JsonRejection>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    require_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    sessions::revoke(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

async fn set_yard_visibility(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<SetYardVisibilityRequest>, JsonRejection>,
) -> Result<Json<Success<YardVisibilityResponse>>, ApiError> {
    require_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    access::set_visibility(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

async fn grant_yard_access(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<GrantYardAccessRequest>, JsonRejection>,
) -> Result<Json<Success<YardAccessGrantResponse>>, ApiError> {
    require_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    access::grant(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

async fn revoke_yard_access(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<RevokeYardAccessRequest>, JsonRejection>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    require_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    access::revoke(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

async fn start_yard_deploy(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<StartYardDeployRequest>, JsonRejection>,
) -> Result<Json<Success<StartYardDeployResponse>>, ApiError> {
    require_deploy(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    deploy::start(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

async fn finalise_yard_deploy(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<YardDeployMutationRequest>, JsonRejection>,
) -> Result<Json<Success<YardDeploymentResponse>>, ApiError> {
    require_deploy(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    deploy::finalise(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

async fn fail_yard_deploy(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<FailYardDeployRequest>, JsonRejection>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    require_deploy(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    deploy::fail(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

async fn rollback_yard(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<RollbackWebYardRequest>, JsonRejection>,
) -> Result<Json<Success<YardDeploymentResponse>>, ApiError> {
    principal.require_actions(&[CiAction::YardManage], "yard:manage")?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    lifecycle::rollback(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

async fn delete_yard(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<DeleteWebYardRequest>, JsonRejection>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    principal.require_actions(&[CiAction::YardManage], "yard:manage")?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    lifecycle::delete(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

fn require_read(principal: &Principal) -> Result<(), ApiError> {
    if principal.is_machine() {
        principal.require_actions(&[CiAction::YardManage], "yard:read")
    } else {
        principal.require_any(&["yard:read", "yard:manage"])
    }
}

fn require_deploy(principal: &Principal) -> Result<(), ApiError> {
    principal.require_actions(&[CiAction::Upload, CiAction::YardManage], "yard:manage")
}

fn require_manage(principal: &Principal) -> Result<(), ApiError> {
    if principal.is_machine() {
        Err(ApiError::forbidden())
    } else {
        principal.require("yard:manage")
    }
}

#[cfg(test)]
#[path = "yards_tests.rs"]
mod tests;
