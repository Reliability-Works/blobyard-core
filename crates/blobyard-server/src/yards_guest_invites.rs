use super::{
    access::parse_future_expiry,
    guest_invite_cursor,
    read::{yard_at, yard_by_id},
};
use crate::{
    api::AppState,
    audit,
    auth::Principal,
    error::ApiError,
    response::{Success, success},
};
use axum::{
    Json,
    extract::{
        Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
};
use blobyard_api_client::{
    CreateYardGuestInviteRequest, CreateYardGuestInviteResponse, ListYardGuestInvitesQuery,
    ListYardGuestInvitesResponse, RevokeYardGuestInviteRequest, RevokeYardGuestInviteResponse,
    YardGuestInvite as ApiInvite, YardGuestInviteStatus as ApiStatus,
};
use blobyard_contract::{
    NewAuditEvent, NewYardAccessGrant, NewYardGuestInvite, YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS,
    YardAccessPrincipalKind, YardGuestAuditInvitation, YardGuestInviteRecord,
    YardGuestInviteStatus, yard_guest_audit_metadata,
};
use blobyard_core::{GeneratedSecretKind, SecretString};

pub(super) async fn list_handler(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<ListYardGuestInvitesQuery>, QueryRejection>,
) -> Result<Json<Success<ListYardGuestInvitesResponse>>, ApiError> {
    super::require_manage(&principal)?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    list(&state, &principal, &query)
}

pub(super) async fn create_handler(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<CreateYardGuestInviteRequest>, JsonRejection>,
) -> Result<Json<Success<CreateYardGuestInviteResponse>>, ApiError> {
    super::require_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    create(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

pub(super) async fn revoke_handler(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<RevokeYardGuestInviteRequest>, JsonRejection>,
) -> Result<Json<Success<RevokeYardGuestInviteResponse>>, ApiError> {
    super::require_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    revoke(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

pub(super) fn list(
    state: &AppState,
    principal: &Principal,
    query: &ListYardGuestInvitesQuery,
) -> Result<Json<Success<ListYardGuestInvitesResponse>>, ApiError> {
    let yard = yard_by_id(state, principal, &query.yard_id)?;
    let cursor = guest_invite_cursor::decode(&yard.id, query.cursor.as_deref())?;
    let limit = usize::from(query.limit.unwrap_or(50));
    if !(1..=50).contains(&limit) {
        return Err(ApiError::invalid_request());
    }
    let page = state
        .repository
        .list_yard_guest_invites(&yard.id, cursor.as_ref(), limit)
        .map_err(ApiError::from_repository)?;
    Ok(success(ListYardGuestInvitesResponse {
        items: page
            .items
            .into_iter()
            .map(present)
            .collect::<Result<Vec<_>, _>>()?,
        next_cursor: page
            .next_cursor
            .as_ref()
            .map(|cursor| guest_invite_cursor::encode(&yard.id, cursor)),
    }))
}

pub(super) fn create(
    state: &AppState,
    principal: &Principal,
    request: &CreateYardGuestInviteRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<CreateYardGuestInviteResponse>>, ApiError> {
    let (yard, now) = yard_at(state, principal, &request.yard_id, now)?;
    let expires_at_ms = invitation_expiry(request.expires_at.as_deref(), now)?;
    let token = crate::auth::generate_token(GeneratedSecretKind::YardGuestInvitation);
    let invitation = NewYardGuestInvite {
        id: format!("ygi_{}", uuid::Uuid::new_v4().simple()),
        workspace_id: yard.workspace_id.clone(),
        project_id: yard.project_id.clone(),
        yard_id: yard.id.clone(),
        environment_id: request.environment_id.clone(),
        email: request.email.trim().to_lowercase(),
        token_hash: crate::auth::hash(token.expose_secret()),
        grant_id: format!("yardgrant_{}", uuid::Uuid::new_v4().simple()),
        created_at_ms: now,
        expires_at_ms,
    };
    let grant = NewYardAccessGrant {
        id: invitation.grant_id.clone(),
        yard_id: yard.id.clone(),
        environment_id: request.environment_id.clone(),
        principal_kind: YardAccessPrincipalKind::GuestInvite,
        principal_id: invitation.id.clone(),
        app_roles: request.app_roles.clone(),
        created_at_ms: now,
        created_by_principal: principal.0.id.clone(),
        expires_at_ms: Some(expires_at_ms),
    };
    let event = event(
        &invitation,
        principal.0.id.clone(),
        "yard.guest_invite.created",
        None,
        now,
    );
    let url = invitation_url(state, &yard.host_label, &token, now, expires_at_ms)?;
    let record = state
        .repository
        .create_yard_guest_invite(&invitation, &grant, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(CreateYardGuestInviteResponse {
        invitation: present(record)?,
        invitation_url: url,
    }))
}

pub(super) fn revoke(
    state: &AppState,
    principal: &Principal,
    request: &RevokeYardGuestInviteRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<RevokeYardGuestInviteResponse>>, ApiError> {
    let (yard, now) = yard_at(state, principal, &request.yard_id, now)?;
    let invitation = state
        .repository
        .yard_guest_invite_by_id(&request.invitation_id)
        .map_err(ApiError::from_repository)?;
    if invitation.yard_id != yard.id {
        return Err(ApiError::not_found());
    }
    let event = event(
        &invitation,
        principal.0.id.clone(),
        "yard.guest_invite.revoked",
        invitation.accepted_subject_id.as_deref(),
        now,
    );
    let record = state
        .repository
        .revoke_yard_guest_invite(&yard.id, &invitation.id, now, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(RevokeYardGuestInviteResponse {
        invitation: present(record)?,
    }))
}

fn invitation_expiry(expires_at: Option<&str>, now: u64) -> Result<u64, ApiError> {
    expires_at.map_or_else(
        || {
            now.checked_add(YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS)
                .ok_or_else(ApiError::internal)
        },
        |value| parse_future_expiry(value, now),
    )
}

pub(super) fn invitation_url(
    state: &AppState,
    host_label: &str,
    token: &SecretString,
    now: u64,
    expires_at_ms: u64,
) -> Result<String, ApiError> {
    let continuation = crate::yard_session_contracts::issue_invitation(
        &state.yard_continuation_key,
        host_label,
        "/",
        now,
        expires_at_ms,
    )?;
    let mut url = ApiError::internal_result(url::Url::parse(&state.public_origin))?;
    url.set_path("/account/yard-invite");
    url.query_pairs_mut()
        .append_pair("token", token.expose_secret())
        .append_pair("continuation", continuation.expose_secret());
    Ok(url.into())
}

fn event(
    invitation: &impl YardGuestAuditInvitation,
    actor: String,
    action: &str,
    subject_id: Option<&str>,
    now: u64,
) -> NewAuditEvent {
    audit::event(
        invitation.workspace_id().to_owned(),
        actor,
        action,
        "yard_guest_invite",
        yard_guest_audit_metadata(invitation, subject_id),
        now,
    )
}

pub(super) fn present(record: YardGuestInviteRecord) -> Result<ApiInvite, ApiError> {
    Ok(ApiInvite {
        accepted_at: record
            .accepted_at_ms
            .map(crate::transfer_grants::format_expiry)
            .transpose()?,
        app_roles: record.app_roles,
        created_at: crate::transfer_grants::format_expiry(record.created_at_ms)?,
        email: record.email,
        environment_id: record.environment_id,
        expires_at: crate::transfer_grants::format_expiry(record.expires_at_ms)?,
        id: record.id,
        revoked_at: record
            .revoked_at_ms
            .map(crate::transfer_grants::format_expiry)
            .transpose()?,
        status: match record.status {
            YardGuestInviteStatus::Pending => ApiStatus::Pending,
            YardGuestInviteStatus::Accepted => ApiStatus::Accepted,
            YardGuestInviteStatus::Revoked => ApiStatus::Revoked,
        },
        yard_id: record.yard_id,
    })
}
