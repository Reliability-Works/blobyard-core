use crate::{api::AppState, auth::Principal, error::ApiError, response::Success};
use axum::{
    Json, Router,
    body::Body,
    extract::{
        OriginalUri, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, Method, Response, StatusCode, header},
    routing::{get, post},
};
use blobyard_api_client::{
    DeleteWebYardRequest, EmptyResponse, FailYardDeployRequest, ListWebYardsQuery,
    ListYardDeploysQuery, RollbackWebYardRequest, StartYardDeployRequest, StartYardDeployResponse,
    WebYardSummary, YardDeployMutationRequest, YardDeploySummary, YardDeploymentResponse,
};
use blobyard_contract::CiAction;

#[path = "yards_contracts.rs"]
mod contracts;
#[path = "yards_deploy.rs"]
mod deploy;
#[path = "yards_lifecycle.rs"]
mod lifecycle;
#[path = "yards_presentation.rs"]
mod presentation;
#[path = "yards_read.rs"]
mod read;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/yards", get(list_yards))
        .route("/v1/yards/delete", post(delete_yard))
        .route("/v1/yards/deploys", get(list_yard_deploys))
        .route("/v1/yards/deploys/fail", post(fail_yard_deploy))
        .route("/v1/yards/deploys/finalise", post(finalise_yard_deploy))
        .route("/v1/yards/deploys/start", post(start_yard_deploy))
        .route("/v1/yards/rollback", post(rollback_yard))
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

pub(crate) async fn public_fallback(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
) -> Result<Response<Body>, ApiError> {
    let authority = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let host_label =
        authority.and_then(|value| contracts::public_host_label(&state.web_yard_origin, value));
    let Some(host_label) = host_label else {
        return crate::previews::public_fallback(State(state), OriginalUri(uri), method, headers)
            .await;
    };
    if method != Method::GET && method != Method::HEAD {
        return Err(ApiError::not_found());
    }
    let path = contracts::public_request_path(uri.path())?;
    let target = state
        .repository
        .yard_file_by_host(&host_label, &path)
        .map_err(ApiError::concealed_capability)?;
    let status = if target.not_found_document {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::OK
    };
    crate::download_io::public_site_response_with_status(
        &state,
        &target.object,
        &headers,
        &method,
        status,
    )
    .await
}

#[cfg(test)]
#[path = "yards_tests.rs"]
mod tests;
