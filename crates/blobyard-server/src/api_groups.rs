use crate::api::AppState;
use crate::auth::Principal;
use crate::error::ApiError;
use crate::response::{Success, success};
use axum::extract::{
    Query, State,
    rejection::{JsonRejection, QueryRejection},
};
use axum::routing::{get, post};
use axum::{Json, Router};
use blobyard_api_client::{
    CreateGroupRequest, DeactivateGroupRequest, EmptyResponse, GroupMemberRequest, GroupResponse,
    ListGroupMembersQuery, ListGroupMembersResponse, ListGroupsQuery, ListGroupsResponse,
    RenameGroupRequest,
};

#[path = "api_groups_cursor.rs"]
mod cursor;
#[path = "api_groups_operations.rs"]
mod operations;

const PAGE_SIZE: u32 = 50;
type MemberPayload = Result<Json<GroupMemberRequest>, JsonRejection>;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/groups", get(list).post(create))
        .route("/v1/groups/rename", post(rename))
        .route("/v1/groups/members", get(list_members).post(add_member))
        .route("/v1/groups/members/remove", post(remove_member))
        .route("/v1/groups/deactivate", post(deactivate))
}

async fn list(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<ListGroupsQuery>, QueryRejection>,
) -> Result<Json<Success<ListGroupsResponse>>, ApiError> {
    crate::api_local_users::require_users_manage(&principal)?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    let workspace =
        crate::api_local_users::authorized_workspace(&state, &principal, &query.workspace)?;
    let position = cursor::decode_group(&workspace.id, query.cursor.as_deref())?;
    let page = state
        .repository
        .list_workspace_groups(&workspace.id, position.as_ref(), PAGE_SIZE)
        .map_err(ApiError::from_repository)?;
    operations::list_response(&workspace.id, page)
}

async fn create(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<CreateGroupRequest>, JsonRejection>,
) -> Result<Json<Success<GroupResponse>>, ApiError> {
    crate::api_local_users::require_users_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    operations::create(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

async fn rename(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<RenameGroupRequest>, JsonRejection>,
) -> Result<Json<Success<GroupResponse>>, ApiError> {
    crate::api_local_users::require_users_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    operations::rename(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

async fn list_members(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<ListGroupMembersQuery>, QueryRejection>,
) -> Result<Json<Success<ListGroupMembersResponse>>, ApiError> {
    crate::api_local_users::require_users_manage(&principal)?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    let position = cursor::decode_member(&query.group_id, query.cursor.as_deref())?;
    let page = state
        .repository
        .list_workspace_group_members(
            &principal.0.workspace_id,
            &query.group_id,
            position.as_ref(),
            PAGE_SIZE,
        )
        .map_err(ApiError::from_repository)?;
    Ok(success(ListGroupMembersResponse {
        items: page
            .items
            .into_iter()
            .map(|member| member.user_id)
            .collect(),
        next_cursor: cursor::encode_member_option(&query.group_id, page.next_cursor.as_ref()),
    }))
}

async fn add_member(
    State(state): State<AppState>,
    principal: Principal,
    payload: MemberPayload,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    crate::api_local_users::require_users_manage(&principal)?;
    let request = member_request(payload)?;
    operations::add_member(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

async fn remove_member(
    State(state): State<AppState>,
    principal: Principal,
    payload: MemberPayload,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    crate::api_local_users::require_users_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    operations::remove_member(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

fn member_request(payload: MemberPayload) -> Result<GroupMemberRequest, ApiError> {
    let Json(request) = ApiError::invalid_request_result(payload)?;
    Ok(request)
}

async fn deactivate(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<DeactivateGroupRequest>, JsonRejection>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    crate::api_local_users::require_users_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    operations::deactivate(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

#[cfg(test)]
#[path = "api_groups_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "api_groups_edge_tests.rs"]
mod edge_tests;
