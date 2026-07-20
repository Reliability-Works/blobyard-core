use super::contracts::{
    content_type_class, notification_status, share_download_expiry, share_expiry, share_page_html,
    share_summary, share_url,
};
use crate::{
    api::AppState,
    auth::{Principal, generate_token, hash},
    error::ApiError,
    response::{Page, Success, page, success},
    transfer_grants as grants,
};
use axum::{
    Json,
    body::Body,
    http::{Response, StatusCode, header},
};
use blobyard_api_client::{
    CreateShareRequest, CreateShareResponse, DownloadResponse, EmptyResponse, ListSharesQuery,
    ResolveShareQuery, RevokeShareRequest, ShareMetadata, ShareSummary,
};
use blobyard_contract::{
    AuditValue, NewAuditEvent, NewDownloadGrant, NewShare, ShareRecord, ShareStatus, ShareTarget,
};
use blobyard_core::{GeneratedSecretKind, SecretString};

const SHARE_DOWNLOAD_TTL_MS: u64 = 60_000;

pub(super) fn create_at(
    state: &AppState,
    principal: &Principal,
    request: &CreateShareRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<CreateShareResponse>>, ApiError> {
    let object = crate::objects::resolve_object(state, &principal.0.workspace_id, &request.target)?;
    let project = grants::resolve_project_slugs(
        state,
        &principal.0.workspace_id,
        request.target.workspace_slug(),
        request.target.project_slug(),
    )?;
    grants::authorize_project_binding(&principal.0, &project)?;
    let now = now?;
    let expires_at_ms = share_expiry(now, request.expires.as_deref())?;
    let raw = generate_token(GeneratedSecretKind::ShareCapability);
    let id = format!("share_{}", uuid::Uuid::new_v4().simple());
    let share = NewShare {
        id: id.clone(),
        workspace_id: principal.0.workspace_id.clone(),
        version_id: object.version.id,
        capability_hash: hash(raw.expose_secret()),
        expires_at_ms,
        maximum_downloads: None,
        created_at_ms: now,
    };
    let response = CreateShareResponse {
        id: id.clone(),
        share_url: share_url(&state.public_origin, &raw)?,
        expires_at: grants::format_expiry(expires_at_ms)?,
        notification_status: notification_status(request.notify.as_deref())?,
    };
    state
        .repository
        .create_share(&share, &share_event(principal, "share.created", &id, now))
        .map_err(ApiError::from_repository)?;
    Ok(success(response))
}

pub(super) fn list_at(
    state: &AppState,
    principal: &Principal,
    query: &ListSharesQuery,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<Page<ShareSummary>>>, ApiError> {
    let workspace = state
        .repository
        .workspace_by_slug(&query.workspace)
        .map_err(ApiError::from_repository)?;
    if workspace.id != principal.0.workspace_id {
        return Err(ApiError::not_found());
    }
    let now = now?;
    let records = state
        .repository
        .list_shares(&workspace.id)
        .map_err(ApiError::from_repository)?;
    share_page(records, now)
}

pub(super) fn share_page(
    records: Vec<ShareRecord>,
    now: u64,
) -> Result<Json<Success<Page<ShareSummary>>>, ApiError> {
    let items = records
        .into_iter()
        .map(|share| share_summary(share, now))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(success(page(items)))
}

pub(super) fn resolve_at(
    state: &AppState,
    query: &ResolveShareQuery,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<ShareMetadata>>, ApiError> {
    let target = state
        .repository
        .share_by_capability(&hash(query.token.expose_secret()), now?)
        .map_err(ApiError::concealed_capability)?;
    Ok(success(ShareMetadata {
        filename: target.object.filename,
        size_bytes: target.object.version.size.ok_or_else(ApiError::not_found)?,
        content_type_class: content_type_class(&target.object.content_type).to_owned(),
        expires_at: grants::format_expiry(target.share.expires_at_ms)?,
        download_available: target.share.status == ShareStatus::Active,
    }))
}

pub(super) fn open_at(
    state: &AppState,
    token: Result<SecretString, ApiError>,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    let token = token?;
    let target = state
        .repository
        .share_by_capability(&hash(token.expose_secret()), now?)
        .map_err(ApiError::concealed_capability)?;
    let html = share_page_html(&target, &format!("/s/{}/download", token.expose_secret()))?;
    crate::response::secure_html(
        html,
        "default-src 'none'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'",
    )
}

pub(super) fn download_shared_file_at(
    state: &AppState,
    token: Result<SecretString, ApiError>,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    let token = token?;
    let issued = issue_share_download_at(state, &token, now)?;
    ApiError::internal_result(
        Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, issued.download_url.expose_secret())
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::empty()),
    )
}

pub(super) fn issue_share_download_at(
    state: &AppState,
    token: &SecretString,
    now: Result<u64, ApiError>,
) -> Result<DownloadResponse, ApiError> {
    let now = now?;
    let capability_hash = hash(token.expose_secret());
    let raw_download = generate_token(GeneratedSecretKind::DownloadCapability);
    let target = state
        .repository
        .share_by_capability(&capability_hash, now)
        .map_err(ApiError::concealed_capability)?;
    issue_for_target(state, &capability_hash, &raw_download, &target, now)
}

pub(super) fn issue_for_target(
    state: &AppState,
    capability_hash: &str,
    raw_download: &SecretString,
    target: &ShareTarget,
    now: u64,
) -> Result<DownloadResponse, ApiError> {
    let expires_at_ms =
        share_download_expiry(now, SHARE_DOWNLOAD_TTL_MS, target.share.expires_at_ms)?;
    let grant = NewDownloadGrant {
        version_id: target.object.version.id.clone(),
        capability_hash: hash(raw_download.expose_secret()),
        expires_at_ms,
    };
    let issued = state
        .repository
        .issue_share_download(
            capability_hash,
            now,
            &grant,
            &public_share_event("share.download_issued", &target.share, now),
        )
        .map_err(ApiError::concealed_capability)?;
    crate::objects::download_response(state, &issued.object, raw_download, expires_at_ms)
}

pub(super) fn revoke_at(
    state: &AppState,
    principal: &Principal,
    request: &RevokeShareRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let now = now?;
    state
        .repository
        .revoke_share(
            &request.share_id,
            &principal.0.workspace_id,
            now,
            &share_event(principal, "share.revoked", &request.share_id, now),
        )
        .map_err(ApiError::from_repository)?;
    Ok(success(EmptyResponse::default()))
}

fn share_event(principal: &Principal, action: &str, share_id: &str, now: u64) -> NewAuditEvent {
    audit_event(
        principal.0.id.clone(),
        principal.0.workspace_id.clone(),
        action,
        share_id,
        now,
    )
}

fn public_share_event(action: &str, share: &ShareRecord, now: u64) -> NewAuditEvent {
    audit_event(
        share.id.clone(),
        share.workspace_id.clone(),
        action,
        &share.id,
        now,
    )
}

fn audit_event(
    actor: String,
    workspace_id: String,
    action: &str,
    share_id: &str,
    now: u64,
) -> NewAuditEvent {
    crate::audit::event(
        workspace_id,
        actor,
        action,
        "share",
        vec![(
            "shareId".to_owned(),
            AuditValue::String(share_id.to_owned()),
        )],
        now,
    )
}
