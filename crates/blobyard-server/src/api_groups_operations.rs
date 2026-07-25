use crate::api::AppState;
use crate::auth::Principal;
use crate::error::ApiError;
use crate::response::{Success, success};
use axum::Json;
use blobyard_api_client::{
    CreateGroupRequest, DeactivateGroupRequest, EmptyResponse, GroupMemberRequest, GroupResponse,
    GroupStatus, GroupSummary, ListGroupsResponse, RenameGroupRequest,
};
use blobyard_contract::{
    AuditValue, NewAuditEvent, WorkspaceGroupMemberRecord, WorkspaceGroupPage,
    WorkspaceGroupRecord, WorkspaceGroupStatus, normalize_group_name,
};

pub(super) fn create(
    state: &AppState,
    principal: &Principal,
    request: &CreateGroupRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<GroupResponse>>, ApiError> {
    let workspace =
        crate::api_local_users::authorized_workspace(state, principal, &request.workspace)?;
    let now_ms = now?;
    let group = WorkspaceGroupRecord {
        id: format!("group_{}", uuid::Uuid::new_v4().simple()),
        workspace_id: workspace.id,
        name: normalize_group_name(&request.name).map_err(ApiError::from_repository)?,
        status: WorkspaceGroupStatus::Active,
        member_count: 0,
        created_at_ms: now_ms,
        deactivated_at_ms: None,
    };
    let event = group_event(
        principal,
        "group.created",
        &group.id,
        vec![("name", AuditValue::String(group.name.clone()))],
        now_ms,
    );
    state
        .repository
        .create_workspace_group(&group, &event)
        .map_err(ApiError::from_repository)?;
    response(group)
}

pub(super) fn rename(
    state: &AppState,
    principal: &Principal,
    request: &RenameGroupRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<GroupResponse>>, ApiError> {
    let now_ms = now?;
    let name = normalize_group_name(&request.name).map_err(ApiError::from_repository)?;
    let event = group_event(
        principal,
        "group.renamed",
        &request.group_id,
        vec![("to", AuditValue::String(name.clone()))],
        now_ms,
    );
    let group = state
        .repository
        .rename_workspace_group(&principal.0.workspace_id, &request.group_id, &name, &event)
        .map_err(ApiError::from_repository)?;
    response(group)
}

pub(super) fn add_member(
    state: &AppState,
    principal: &Principal,
    request: &GroupMemberRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let now_ms = now?;
    let member = WorkspaceGroupMemberRecord {
        group_id: request.group_id.clone(),
        workspace_id: principal.0.workspace_id.clone(),
        user_id: request.user_id.clone(),
        added_at_ms: now_ms,
    };
    let event = member_event(principal, "group.member_added", &member, now_ms);
    state
        .repository
        .add_workspace_group_member(&member, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(EmptyResponse::default()))
}

pub(super) fn remove_member(
    state: &AppState,
    principal: &Principal,
    request: &GroupMemberRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let now_ms = now?;
    let event = group_event(
        principal,
        "group.member_removed",
        &request.group_id,
        vec![("userId", AuditValue::String(request.user_id.clone()))],
        now_ms,
    );
    state
        .repository
        .remove_workspace_group_member(
            &principal.0.workspace_id,
            &request.group_id,
            &request.user_id,
            &event,
        )
        .map_err(ApiError::from_repository)?;
    Ok(success(EmptyResponse::default()))
}

pub(super) fn deactivate(
    state: &AppState,
    principal: &Principal,
    request: &DeactivateGroupRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let now_ms = now?;
    let event = group_event(
        principal,
        "group.deactivated",
        &request.group_id,
        Vec::new(),
        now_ms,
    );
    state
        .repository
        .deactivate_workspace_group(&principal.0.workspace_id, &request.group_id, now_ms, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(EmptyResponse::default()))
}

pub(super) fn summaries(groups: Vec<WorkspaceGroupRecord>) -> Result<Vec<GroupSummary>, ApiError> {
    groups.into_iter().map(summary).collect()
}

pub(super) fn list_response(
    workspace_id: &str,
    page: WorkspaceGroupPage,
) -> Result<Json<Success<ListGroupsResponse>>, ApiError> {
    Ok(success(ListGroupsResponse {
        items: summaries(page.items)?,
        next_cursor: super::cursor::encode_group_option(workspace_id, page.next_cursor.as_ref()),
    }))
}

pub(super) fn response(
    group: WorkspaceGroupRecord,
) -> Result<Json<Success<GroupResponse>>, ApiError> {
    Ok(success(GroupResponse {
        group: summary(group)?,
    }))
}

fn member_event(
    principal: &Principal,
    action: &str,
    member: &WorkspaceGroupMemberRecord,
    now_ms: u64,
) -> NewAuditEvent {
    group_event(
        principal,
        action,
        &member.group_id,
        vec![("userId", AuditValue::String(member.user_id.clone()))],
        now_ms,
    )
}

fn group_event(
    principal: &Principal,
    action: &str,
    group_id: &str,
    mut metadata: Vec<(&str, AuditValue)>,
    now_ms: u64,
) -> NewAuditEvent {
    metadata.push(("groupId", AuditValue::String(group_id.to_owned())));
    metadata.push((
        "workspaceId",
        AuditValue::String(principal.0.workspace_id.clone()),
    ));
    crate::audit::event(
        principal.0.workspace_id.clone(),
        principal.0.id.clone(),
        action,
        "workspace_group",
        metadata
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
        now_ms,
    )
}

fn summary(group: WorkspaceGroupRecord) -> Result<GroupSummary, ApiError> {
    Ok(GroupSummary {
        id: group.id,
        workspace_id: group.workspace_id,
        name: group.name,
        status: match group.status {
            WorkspaceGroupStatus::Active => GroupStatus::Active,
            WorkspaceGroupStatus::Deactivated => GroupStatus::Deactivated,
        },
        created_at: crate::transfer_grants::format_expiry(group.created_at_ms)?,
        deactivated_at: group
            .deactivated_at_ms
            .map(crate::transfer_grants::format_expiry)
            .transpose()?,
        member_count: group.member_count,
    })
}
