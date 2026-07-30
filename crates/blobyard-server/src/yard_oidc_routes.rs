use crate::{
    api::AppState,
    error::ApiError,
    inboxes::PeerFingerprint,
    yard_account, yard_login, yard_oidc_contracts,
    yard_oidc_provider::{YardOidcAuthorization, YardOidcProviderError},
    yard_session_contracts,
};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{Method, Request, Response, StatusCode},
    routing::any,
};
use blobyard_contract::{
    NewYardContinuation, NewYardOidcAuthentication, RepositoryError,
    YARD_EXCHANGE_CODE_LIFETIME_MS, YardOidcAuditContext,
};
use blobyard_core::{GeneratedSecretKind, SecretString};
use std::sync::Arc;

fn provider(state: &AppState) -> Option<&Arc<dyn crate::yard_oidc_provider::YardOidcProvider>> {
    state.yard_oidc_provider.as_ref()
}

fn repository_lookup<T>(result: Result<T, RepositoryError>) -> Result<Option<T>, ApiError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(RepositoryError::NotFound | RepositoryError::InvalidInput) => Ok(None),
        Err(_) => Err(ApiError::internal()),
    }
}

const fn repository_write(result: Result<(), RepositoryError>) -> Result<bool, ApiError> {
    match result {
        Ok(()) => Ok(true),
        Err(RepositoryError::Conflict | RepositoryError::NotFound) => Ok(false),
        Err(_) => Err(ApiError::internal()),
    }
}

fn start_redirect(
    location: &str,
    result: Result<(), RepositoryError>,
) -> Result<Response<Body>, ApiError> {
    if repository_write(result)? {
        crate::response::redirect(StatusCode::SEE_OTHER, location, None)
    } else {
        Ok(crate::yard_login::page::invalid_link())
    }
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/account/yard-oidc/start", any(start_dispatch))
        .route("/account/yard-oidc/callback", any(callback_dispatch))
}

async fn start_dispatch(
    State(state): State<AppState>,
    PeerFingerprint(fingerprint): PeerFingerprint,
    request: Request<Body>,
) -> Result<Response<Body>, ApiError> {
    if *request.method() != Method::POST {
        yard_account::require_identity_host(&state, request.headers())?;
        return Ok(crate::yard_login::page::invalid_link());
    }
    yard_account::require_identity_request(&state, &request)?;
    start(
        &state,
        &fingerprint,
        request,
        crate::transfer_grants::now_ms(),
    )
    .await
}

async fn start(
    state: &AppState,
    fingerprint: &str,
    request: Request<Body>,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    let Some(provider) = provider(state) else {
        return Ok(crate::yard_login::page::invalid_link());
    };
    let Some(body) = yard_account::form_body(request).await? else {
        return Ok(crate::yard_login::page::invalid_link());
    };
    let Some(continuation) = yard_login::single_parameter(&body, "continuation")
        .and_then(|value| SecretString::new(value).ok())
    else {
        return Ok(crate::yard_login::page::invalid_link());
    };
    let now = now?;
    let Ok(claims) =
        yard_session_contracts::verify(&state.yard_continuation_key, &continuation, now)
    else {
        return Ok(crate::yard_login::page::invalid_link());
    };
    yard_account::consume_rate_limit(state, fingerprint, "yard-oidc-start", now)?;
    let raw_state = yard_oidc_contracts::generate_state();
    let attempt = yard_oidc_contracts::attempt(
        &raw_state,
        &continuation,
        claims.host_label(),
        claims.return_path(),
        now,
    );
    let derived = yard_oidc_contracts::derive(&state.yard_continuation_key, &raw_state);
    let authorization = YardOidcAuthorization {
        state: raw_state,
        nonce: derived.nonce,
        pkce_verifier: derived.pkce_verifier,
    };
    let location = provider
        .authorization_url(&authorization)
        .map_err(|_error| ApiError::internal())?;
    start_redirect(
        &location,
        state.repository.create_yard_oidc_attempt(&attempt),
    )
}

async fn callback_dispatch(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<Response<Body>, ApiError> {
    yard_account::require_identity_host(&state, request.headers())?;
    if *request.method() != Method::GET {
        return Ok(crate::yard_login::page::invalid_link());
    }
    callback(
        &state,
        request.uri().query(),
        crate::transfer_grants::now_ms(),
    )
    .await
}

async fn callback(
    state: &AppState,
    query: Option<&str>,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    let Some((code, raw_state)) =
        yard_account::exact_parameters(query.unwrap_or_default(), "code", "state")
    else {
        return Ok(crate::yard_login::page::invalid_link());
    };
    let Some(provider) = provider(state) else {
        return Ok(crate::yard_login::page::invalid_link());
    };
    let Some((code, raw_state)) = SecretString::new(code)
        .ok()
        .zip(SecretString::new(raw_state).ok())
        .filter(|(_code, state)| yard_oidc_contracts::state_shape(state.expose_secret()))
    else {
        return Ok(crate::yard_login::page::invalid_link());
    };
    let now = now?;
    let state_hash = crate::auth::hash(raw_state.expose_secret());
    let Some(attempt) =
        repository_lookup(state.repository.claim_yard_oidc_attempt(&state_hash, now))?
    else {
        return Ok(crate::yard_login::page::invalid_link());
    };
    let derived = yard_oidc_contracts::derive(&state.yard_continuation_key, &raw_state);
    let identity = match provider
        .exchange(code, derived.nonce, derived.pkce_verifier, now)
        .await
    {
        Ok(identity) => identity,
        Err(YardOidcProviderError::Unavailable | YardOidcProviderError::InvalidResponse) => {
            return Ok(crate::yard_login::page::invalid_link());
        }
    };
    authenticate_and_redirect(state, &attempt, identity, now)
}

fn authenticate_and_redirect(
    state: &AppState,
    attempt: &blobyard_contract::YardOidcAttemptRecord,
    identity: crate::yard_oidc_provider::YardOidcVerifiedIdentity,
    now: u64,
) -> Result<Response<Body>, ApiError> {
    let authentication = NewYardOidcAuthentication {
        issuer: identity.issuer,
        provider_subject: identity.provider_subject,
        normalized_email: identity.normalized_email,
        host_label: attempt.attempt.host_label.clone(),
        authenticated_at_ms: now,
    };
    let audit = YardOidcAuditContext {
        id: format!("audit_{}", uuid::Uuid::new_v4().simple()),
        request_id: format!("request_{}", uuid::Uuid::new_v4().simple()),
    };
    let Some(binding) = repository_lookup(
        state
            .repository
            .authenticate_yard_oidc_identity(&authentication, &audit),
    )?
    else {
        return Ok(crate::yard_login::page::invalid_link());
    };
    let subject = binding.yard_subject_id;
    issue_redirect(state, &attempt.attempt, &subject, now)
}

fn issue_redirect(
    state: &AppState,
    attempt: &blobyard_contract::NewYardOidcAttempt,
    subject_id: &str,
    now: u64,
) -> Result<Response<Body>, ApiError> {
    let Some(admission) = repository_lookup(state.repository.evaluate_yard_admission(
        &attempt.host_label,
        subject_id,
        now,
    ))?
    else {
        return Ok(crate::yard_login::page::invalid_link());
    };
    let code = crate::auth::generate_token(GeneratedSecretKind::YardExchangeCode);
    let expires_at_ms = exchange_expiry(now);
    let durable = NewYardContinuation {
        id: format!("yardcont_{}", uuid::Uuid::new_v4().simple()),
        continuation_hash: attempt.continuation_hash.clone(),
        code_hash: crate::auth::hash(code.expose_secret()),
        yard_id: admission.yard_id,
        environment_id: admission.environment_id,
        host_label: attempt.host_label.clone(),
        user_id: subject_id.to_owned(),
        return_path: attempt.return_path.clone(),
        created_at_ms: now,
        expires_at_ms,
    };
    if repository_write(state.repository.issue_yard_exchange_code(&durable))? {
        yard_login::exchange_redirect(&state.web_yard_origin, &attempt.host_label, &code)
    } else {
        Ok(crate::yard_login::page::invalid_link())
    }
}

const fn exchange_expiry(now: u64) -> u64 {
    now.saturating_add(YARD_EXCHANGE_CODE_LIFETIME_MS)
}

#[cfg(test)]
#[path = "yard_oidc_routes_admission_edge_tests.rs"]
mod admission_edge_tests;
#[cfg(test)]
#[path = "yard_oidc_routes_edge_tests.rs"]
mod edge_tests;
#[cfg(test)]
#[path = "yard_oidc_routes_guest_test_support.rs"]
mod guest_test_support;
#[cfg(test)]
#[path = "yard_oidc_routes_test_support.rs"]
mod test_support;
#[cfg(test)]
#[path = "yard_oidc_routes_tests.rs"]
mod tests;
