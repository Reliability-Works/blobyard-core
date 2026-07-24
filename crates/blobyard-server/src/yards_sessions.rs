use super::read::authorize_yard;
use crate::{
    api::AppState,
    audit,
    auth::Principal,
    error::ApiError,
    response::{Success, success},
};
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
    let yard = state
        .repository
        .web_yard_by_id(&query.yard_id)
        .map_err(ApiError::from_repository)?;
    authorize_yard(principal, &yard)?;
    let now = now?;
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
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let yard = state
        .repository
        .web_yard_by_id(&request.yard_id)
        .map_err(ApiError::from_repository)?;
    authorize_yard(principal, &yard)?;
    let now = now?;
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

fn summary(listing: YardSessionListing, now_ms: u64) -> Result<YardSessionSummary, ApiError> {
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

const fn status(status: YardSessionStatus) -> ApiStatus {
    match status {
        YardSessionStatus::Active => ApiStatus::Active,
        YardSessionStatus::Expired => ApiStatus::Expired,
        YardSessionStatus::Revoked => ApiStatus::Revoked,
    }
}
