use super::contracts::{
    formatted_time, manifest_root, preview_expiry, preview_url, snapshot_manifest, status,
};
use crate::{
    api::AppState,
    auth::{Principal, generate_preview_host_capability, hash},
    error::ApiError,
    response::{Page, Success, page, success},
    transfer_grants as grants,
};
use axum::Json;
use blobyard_api_client::{
    CreatePreviewRequest, CreatePreviewResponse, EmptyResponse, ListPreviewsQuery, PreviewSummary,
    RevokePreviewRequest,
};
use blobyard_contract::{AuditValue, NewAuditEvent, NewPreview, PreviewRecord};

pub(super) fn create_at(
    state: &AppState,
    principal: &Principal,
    request: &CreatePreviewRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<CreatePreviewResponse>>, ApiError> {
    let project = grants::resolve_authorized_project(
        state,
        &principal.0,
        &request.workspace,
        &request.project,
    )?;
    let root = manifest_root(&request.manifest_id)?;
    let objects = state
        .repository
        .list_stored_objects(&project.id, Some(&root), false)
        .map_err(ApiError::from_repository)?;
    let files = snapshot_manifest(&root, objects)?;
    let now = now?;
    let expires_at_ms = preview_expiry(now, request.expires.as_deref())?;
    let capability = generate_preview_host_capability();
    let id = format!("preview_{}", uuid::Uuid::new_v4().simple());
    let preview = NewPreview {
        id: id.clone(),
        workspace_id: principal.0.workspace_id.clone(),
        project_id: project.id,
        capability_hash: hash(capability.expose_secret()),
        expires_at_ms,
        created_at_ms: now,
        files,
    };
    let response = CreatePreviewResponse {
        id: id.clone(),
        preview_url: preview_url(&state.web_yard_origin, &capability)?,
        expires_at: formatted_time(expires_at_ms)?,
    };
    state
        .repository
        .create_preview(
            &preview,
            &preview_event(principal, "preview.created", &id, now),
        )
        .map_err(ApiError::from_repository)?;
    Ok(success(response))
}

pub(super) fn list_at(
    state: &AppState,
    principal: &Principal,
    query: &ListPreviewsQuery,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<Page<PreviewSummary>>>, ApiError> {
    let project =
        grants::resolve_authorized_project(state, &principal.0, &query.workspace, &query.project)?;
    let now = now?;
    let records = state
        .repository
        .list_previews(&project.id)
        .map_err(ApiError::from_repository)?;
    Ok(success(page(
        records
            .into_iter()
            .map(|record| summary(record, now))
            .collect::<Result<Vec<_>, _>>()?,
    )))
}

pub(super) fn revoke_at(
    state: &AppState,
    principal: &Principal,
    request: &RevokePreviewRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let preview = state
        .repository
        .preview_by_id(&request.preview_id)
        .map_err(ApiError::from_repository)?;
    authorize_preview(principal, &preview)?;
    let now = now?;
    state
        .repository
        .revoke_preview(
            &preview.id,
            &preview.workspace_id,
            &preview.project_id,
            now,
            &preview_event(principal, "preview.revoked", &preview.id, now),
        )
        .map_err(ApiError::from_repository)?;
    Ok(success(EmptyResponse::default()))
}

pub(super) fn authorize_preview(
    principal: &Principal,
    preview: &PreviewRecord,
) -> Result<(), ApiError> {
    let workspace_matches = preview.workspace_id == principal.0.workspace_id;
    let project_matches = principal
        .0
        .project_id
        .as_ref()
        .is_none_or(|project_id| project_id == &preview.project_id);
    if workspace_matches && project_matches {
        Ok(())
    } else {
        Err(ApiError::not_found())
    }
}

fn summary(record: PreviewRecord, now: u64) -> Result<PreviewSummary, ApiError> {
    let current_status = status(&record, now).to_owned();
    Ok(PreviewSummary {
        id: record.id,
        created_at: formatted_time(record.created_at_ms)?,
        expires_at: formatted_time(record.expires_at_ms)?,
        status: current_status,
    })
}

fn preview_event(principal: &Principal, action: &str, preview_id: &str, now: u64) -> NewAuditEvent {
    crate::audit::event(
        principal.0.workspace_id.clone(),
        principal.0.id.clone(),
        action,
        "preview",
        vec![(
            "previewId".to_owned(),
            AuditValue::String(preview_id.to_owned()),
        )],
        now,
    )
}
