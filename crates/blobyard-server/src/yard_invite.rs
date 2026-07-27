use crate::{api::AppState, error::ApiError, inboxes::PeerFingerprint, yard_account};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{Method, Request, Response},
    routing::any,
};

#[path = "yard_invite_accept.rs"]
mod accept;
#[path = "yard_invite_page.rs"]
mod page;
#[path = "yard_invite_resolution.rs"]
mod resolution;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/account/yard-invite", any(invitation))
        .route("/account/yard-invite/accept", any(acceptance))
}

async fn invitation(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<Response<Body>, ApiError> {
    invitation_at(&state, request, crate::transfer_grants::now_ms())
}

fn invitation_at(
    state: &AppState,
    request: Request<Body>,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    let (parts, _body) = request.into_parts();
    yard_account::require_identity_host(state, &parts.headers)?;
    if parts.method != Method::GET {
        return Err(ApiError::not_found());
    }
    let now = now?;
    let Some((token, continuation, claims, invite)) =
        resolution::query(state, parts.uri.query(), now)?
    else {
        return Ok(page::invalid_link());
    };
    Ok(page::invitation(
        &invite.email,
        token.expose_secret(),
        continuation.expose_secret(),
        claims.host_label(),
    ))
}

async fn acceptance(
    State(state): State<AppState>,
    PeerFingerprint(fingerprint): PeerFingerprint,
    request: Request<Body>,
) -> Result<Response<Body>, ApiError> {
    acceptance_at(
        &state,
        &fingerprint,
        request,
        crate::transfer_grants::now_ms(),
    )
    .await
}

async fn acceptance_at(
    state: &AppState,
    fingerprint: &str,
    request: Request<Body>,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    yard_account::require_identity_request(state, &request)?;
    if request.method() != Method::POST {
        return Err(ApiError::not_found());
    }
    let form = parse_request(request).await?;
    let now = now?;
    consume_rate_limit(state, fingerprint, now)?;
    let Some((token, continuation)) = form else {
        return Ok(page::invalid_link());
    };
    let Some((token, continuation, claims, invitation)) =
        resolution::values(state, token, continuation, now)?
    else {
        return Ok(page::invalid_link());
    };
    accept::invitation(state, &token, &continuation, &claims, &invitation, now)
}

async fn parse_request(request: Request<Body>) -> Result<Option<(String, String)>, ApiError> {
    let body = yard_account::form_body(request)
        .await?
        .ok_or_else(ApiError::invalid_request)?;
    Ok(accept_form(&body))
}

fn consume_rate_limit(state: &AppState, fingerprint: &str, now: u64) -> Result<(), ApiError> {
    yard_account::consume_rate_limit(state, fingerprint, "yard-invite", now)
}

fn invite_parameters(input: &str) -> Option<(String, String)> {
    yard_account::exact_parameters(input, "token", "continuation")
}

fn accept_form(input: &str) -> Option<(String, String)> {
    yard_account::exact_parameters(input, "token", "continuation")
}

#[cfg(test)]
#[path = "yard_invite_test_support.rs"]
mod test_support;
#[cfg(test)]
#[path = "yard_invite_tests.rs"]
mod tests;
