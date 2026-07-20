use super::contracts::{
    MAXIMUM_BYTES, MAXIMUM_FILES, RATE_WINDOW_MS, RESOLVE_RATE_LIMIT, expiry, inbox_url, metadata,
    resolve_rate_key, summary,
};
use crate::api::AppState;
use crate::auth::{Principal, generate_token, hash};
use crate::error::ApiError;
use crate::response::{Page, Success, page, success};
use crate::transfer_grants as grants;
use axum::Json;
use blobyard_api_client::{
    CreateInboxRequest, CreateInboxResponse, EmptyResponse, InboxMetadata, InboxSummary,
    ListInboxesQuery, ResolveInboxQuery, RevokeInboxRequest,
};
use blobyard_contract::{AuditValue, NewAuditEvent, NewInbox};
use blobyard_core::GeneratedSecretKind;

pub(super) fn require_manager(principal: &Principal) -> Result<(), ApiError> {
    if principal.is_machine() {
        Err(ApiError::forbidden())
    } else {
        principal.require("inbox:manage")
    }
}

pub(super) fn create_at(
    state: &AppState,
    principal: &Principal,
    request: &CreateInboxRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<CreateInboxResponse>>, ApiError> {
    crate::slug::validate_name(&request.name)?;
    let project = grants::resolve_authorized_project(
        state,
        &principal.0,
        &request.workspace,
        &request.project,
    )?;
    let now = now?;
    let expires_at_ms = expiry(now, request.expires.as_deref())?;
    let raw = generate_token(GeneratedSecretKind::InboxCapability);
    let id = format!("inbox_{}", uuid::Uuid::new_v4().simple());
    let inbox = NewInbox {
        id: id.clone(),
        workspace_id: principal.0.workspace_id.clone(),
        project_id: project.id,
        name: request.name.clone(),
        capability_hash: hash(raw.expose_secret()),
        expires_at_ms,
        maximum_files: MAXIMUM_FILES,
        maximum_bytes: MAXIMUM_BYTES,
        created_at_ms: now,
    };
    let response = CreateInboxResponse {
        id: id.clone(),
        inbox_url: inbox_url(&state.public_origin, &raw)?,
        expires_at: grants::format_expiry(expires_at_ms)?,
    };
    state
        .repository
        .create_inbox(&inbox, &event(principal, "inbox.created", &id, now))
        .map_err(ApiError::from_repository)?;
    Ok(success(response))
}

pub(super) fn list_at(
    state: &AppState,
    principal: &Principal,
    query: &ListInboxesQuery,
) -> Result<Json<Success<Page<InboxSummary>>>, ApiError> {
    if query.cursor.is_some() {
        return Err(ApiError::invalid_request());
    }
    let project =
        grants::resolve_authorized_project(state, &principal.0, &query.workspace, &query.project)?;
    let records = state
        .repository
        .list_inboxes(&project.id)
        .map_err(ApiError::from_repository)?;
    let items = records
        .into_iter()
        .map(summary)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(success(page(items)))
}

pub(super) fn resolve_at(
    state: &AppState,
    query: &ResolveInboxQuery,
    now: Result<u64, ApiError>,
    fingerprint: &str,
) -> Result<Json<Success<InboxMetadata>>, ApiError> {
    Ok(success(resolve_metadata_at(
        state,
        query,
        now,
        fingerprint,
    )?))
}

pub(super) fn resolve_metadata_at(
    state: &AppState,
    query: &ResolveInboxQuery,
    now: Result<u64, ApiError>,
    fingerprint: &str,
) -> Result<InboxMetadata, ApiError> {
    let now = now?;
    let token_hash = hash(query.token.expose_secret());
    let rate_key = resolve_rate_key(&token_hash, fingerprint);
    crate::inbox_rate::consume(state, &rate_key, RATE_WINDOW_MS, RESOLVE_RATE_LIMIT, now)?;
    let record = state
        .repository
        .inbox_by_capability(&token_hash, now)
        .map_err(ApiError::concealed_capability)?;
    metadata(record)
}

pub(super) fn revoke_at(
    state: &AppState,
    principal: &Principal,
    request: &RevokeInboxRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let now = now?;
    state
        .repository
        .revoke_inbox(
            &request.inbox_id,
            &principal.0.workspace_id,
            now,
            &event(principal, "inbox.revoked", &request.inbox_id, now),
        )
        .map_err(ApiError::from_repository)?;
    Ok(success(EmptyResponse::default()))
}

fn event(principal: &Principal, action: &str, inbox_id: &str, created_at_ms: u64) -> NewAuditEvent {
    crate::audit::event(
        principal.0.workspace_id.clone(),
        principal.0.id.clone(),
        action,
        "inbox",
        vec![(
            "inboxId".to_owned(),
            AuditValue::String(inbox_id.to_owned()),
        )],
        created_at_ms,
    )
}
