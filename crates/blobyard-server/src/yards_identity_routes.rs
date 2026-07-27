use super::{identity, require_manage};
use crate::{api::AppState, auth::Principal, error::ApiError, response::Success};
use axum::{
    Json,
    extract::{
        Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
};
use blobyard_api_client::{
    EmptyResponse, GetYardApplicationPolicyQuery, ListYardManagementRolesQuery,
    ListYardManagementRolesResponse, RevokeYardManagementRoleRequest, SetYardAccessRolesRequest,
    SetYardAccessRolesResponse, SetYardApplicationPolicyRequest, SetYardManagementRoleRequest,
    YardApplicationPolicyResponse, YardManagementRoleAssignment,
};

pub(super) async fn list_yard_management_roles(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<ListYardManagementRolesQuery>, QueryRejection>,
) -> Result<Json<Success<ListYardManagementRolesResponse>>, ApiError> {
    require_manage(&principal)?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    identity::list_management_roles(&state, &principal, &query)
}

pub(super) async fn set_yard_management_role(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<SetYardManagementRoleRequest>, JsonRejection>,
) -> Result<Json<Success<YardManagementRoleAssignment>>, ApiError> {
    require_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    identity::set_management_role(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

pub(super) async fn revoke_yard_management_role(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<RevokeYardManagementRoleRequest>, JsonRejection>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    require_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    identity::revoke_management_role(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

pub(super) async fn get_yard_application_policy(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<GetYardApplicationPolicyQuery>, QueryRejection>,
) -> Result<Json<Success<YardApplicationPolicyResponse>>, ApiError> {
    require_manage(&principal)?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    identity::get_application_policy(&state, &principal, &query)
}

pub(super) async fn set_yard_application_policy(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<SetYardApplicationPolicyRequest>, JsonRejection>,
) -> Result<Json<Success<YardApplicationPolicyResponse>>, ApiError> {
    require_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    identity::set_application_policy(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

pub(super) async fn set_yard_access_roles(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<SetYardAccessRolesRequest>, JsonRejection>,
) -> Result<Json<Success<SetYardAccessRolesResponse>>, ApiError> {
    require_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    identity::set_access_roles(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}
