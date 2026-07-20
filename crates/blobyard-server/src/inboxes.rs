use crate::{api::AppState, auth::Principal, error::ApiError, response::Success};
use axum::{
    Json, Router,
    extract::{
        ConnectInfo, FromRequestParts, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::request::Parts,
    routing::{get, post},
};
use blobyard_api_client::{
    CreateInboxRequest, CreateInboxResponse, EmptyResponse, InboxMetadata, InboxSummary,
    ListInboxesQuery, ResolveInboxQuery, RevokeInboxRequest,
};
use std::net::SocketAddr;

#[path = "inboxes_contracts.rs"]
mod contracts;

#[path = "inboxes_operations.rs"]
mod operations;

#[path = "inbox_browser.rs"]
mod browser;

struct PeerFingerprint(String);

impl FromRequestParts<AppState> for PeerFingerprint {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|value| value.0);
        Ok(Self(contracts::peer_fingerprint(peer)))
    }
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/inboxes", get(list_inboxes).post(create_inbox))
        .route("/v1/inboxes/resolve", get(resolve_inbox))
        .route("/v1/inboxes/revoke", post(revoke_inbox))
        .route("/i/{token}", get(browser::open))
        .route("/assets/inbox-upload.js", get(browser::script))
}

async fn create_inbox(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<CreateInboxRequest>, JsonRejection>,
) -> Result<Json<Success<CreateInboxResponse>>, ApiError> {
    operations::require_manager(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    operations::create_at(
        &state,
        &principal,
        &request,
        crate::transfer_grants::now_ms(),
    )
}

async fn list_inboxes(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<ListInboxesQuery>, QueryRejection>,
) -> Result<Json<Success<crate::response::Page<InboxSummary>>>, ApiError> {
    operations::require_manager(&principal)?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    operations::list_at(&state, &principal, &query)
}

async fn resolve_inbox(
    State(state): State<AppState>,
    PeerFingerprint(fingerprint): PeerFingerprint,
    query: Result<Query<ResolveInboxQuery>, QueryRejection>,
) -> Result<Json<Success<InboxMetadata>>, ApiError> {
    let Query(query) = ApiError::not_found_result(query)?;
    operations::resolve_at(
        &state,
        &query,
        crate::transfer_grants::now_ms(),
        &fingerprint,
    )
}

async fn revoke_inbox(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<RevokeInboxRequest>, JsonRejection>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    operations::require_manager(&principal)?;
    let request = ApiError::invalid_request_result(payload)?.0;
    let now = crate::transfer_grants::now_ms();
    operations::revoke_at(&state, &principal, &request, now)
}

#[cfg(test)]
#[path = "inboxes_tests.rs"]
mod tests;
