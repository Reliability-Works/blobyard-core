use super::EmptyMutation;
use super::read::yard_at;
use crate::api::AppState;
use crate::audit;
use crate::auth::Principal;
use crate::error::ApiError;
use crate::response::{Success, success};
use axum::Json;
use blobyard_api_client::{
    EmptyResponse, ListYardSessionsQuery, ListYardSessionsResponse, RevokeYardSessionRequest,
    YardSessionStatus as ApiStatus, YardSessionSummary,
};
use blobyard_contract::{AuditValue, YardSessionListing, YardSessionStatus};

pub(super) fn list(
    state: &AppState,
    principal: &Principal,
    query: &ListYardSessionsQuery,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<ListYardSessionsResponse>>, ApiError> {
    let (yard, now) = yard_at(state, principal, &query.yard_id, now)?;
    let sessions = state
        .repository
        .list_yard_sessions(&yard.id)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .map(|listing| summary(listing, now))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(success(ListYardSessionsResponse { sessions }))
}

pub(super) fn revoke(
    state: &AppState,
    principal: &Principal,
    request: &RevokeYardSessionRequest,
    now: Result<u64, ApiError>,
) -> EmptyMutation {
    let (yard, now) = yard_at(state, principal, &request.yard_id, now)?;
    let event = audit::event(
        yard.workspace_id,
        principal.0.id.clone(),
        "yard.session_revoked",
        "yard_session",
        vec![
            (
                "sessionId".to_owned(),
                AuditValue::String(request.session_id.clone()),
            ),
            (
                "yardId".to_owned(),
                AuditValue::String(request.yard_id.clone()),
            ),
        ],
        now,
    );
    state
        .repository
        .revoke_yard_session(&request.yard_id, &request.session_id, now, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(EmptyResponse::default()))
}

pub(super) fn summary(
    listing: YardSessionListing,
    now_ms: u64,
) -> Result<YardSessionSummary, ApiError> {
    let session = listing.session;
    let effective_status = session.status_at(now_ms);
    Ok(YardSessionSummary {
        created_at: crate::transfer_grants::format_expiry(session.created_at_ms)?,
        environment_id: session.environment_id,
        expires_at: crate::transfer_grants::format_expiry(session.expires_at_ms)?,
        host_label: session.host_label,
        id: session.id,
        last_used_at: session
            .last_used_at_ms
            .map(crate::transfer_grants::format_expiry)
            .transpose()?,
        status: status(effective_status),
        user_display_name: listing.user_display_name,
        user_id: session.user_id,
        yard_id: session.yard_id,
    })
}

pub(super) const fn status(status: YardSessionStatus) -> ApiStatus {
    match status {
        YardSessionStatus::Active => ApiStatus::Active,
        YardSessionStatus::Expired => ApiStatus::Expired,
        YardSessionStatus::Revoked => ApiStatus::Revoked,
    }
}
