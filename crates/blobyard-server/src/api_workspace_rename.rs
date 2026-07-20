use crate::{
    api::{AppState, WorkspaceResponse},
    auth::Principal,
    error::ApiError,
    response::{Success, success},
    slug, transfer_grants,
};
use axum::{
    Json, Router,
    extract::{State, rejection::JsonRejection},
    routing::post,
};
use blobyard_contract::WorkspaceRecord;
use blobyard_core::Slug;
use serde::Deserialize;

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/v1/workspaces/rename", post(rename))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RenameRequest {
    name: String,
    workspace: Slug,
}

async fn rename(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<RenameRequest>, JsonRejection>,
) -> Result<Json<Success<WorkspaceResponse>>, ApiError> {
    let Json(request) = ApiError::invalid_request_result(payload)?;
    rename_with_clock(&state, &principal, request, transfer_grants::now_ms())
}

fn rename_with_clock(
    state: &AppState,
    principal: &Principal,
    request: RenameRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<WorkspaceResponse>>, ApiError> {
    principal.require("project:write")?;
    slug::validate_name(&request.name)?;
    let current = state
        .repository
        .workspace_by_slug(&request.workspace)
        .map_err(ApiError::from_repository)?;
    if current.id != principal.0.workspace_id {
        return Err(ApiError::not_found());
    }
    let renamed_slug = slug::from_name(&request.name).ok_or_else(ApiError::invalid_request)?;
    let renamed = WorkspaceRecord {
        id: current.id,
        name: request.name,
        slug: renamed_slug,
    };
    let event = crate::audit::workspace_renamed_event(&principal.0, &current.slug, now?);
    state
        .repository
        .rename_workspace(&renamed, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(WorkspaceResponse::from(renamed)))
}

#[cfg(test)]
#[path = "api_workspace_rename_tests.rs"]
mod tests;
