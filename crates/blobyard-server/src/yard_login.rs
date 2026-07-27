use crate::{
    api::AppState,
    error::ApiError,
    inboxes::PeerFingerprint,
    yard_account,
    yard_session_contracts::{self, ContinuationClaims},
};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{Method, Request, Response, StatusCode},
    routing::any,
};
use blobyard_contract::{NewYardContinuation, RepositoryError, YARD_EXCHANGE_CODE_LIFETIME_MS};
use blobyard_core::{GeneratedSecretKind, SecretString};

#[path = "yard_login_page.rs"]
mod page;

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/account/yard-login", any(dispatch))
}

async fn dispatch(
    State(state): State<AppState>,
    PeerFingerprint(fingerprint): PeerFingerprint,
    request: Request<Body>,
) -> Result<Response<Body>, ApiError> {
    yard_account::require_identity_host(&state, request.headers())?;
    match *request.method() {
        Method::GET => get(&state, request.uri().query()),
        Method::POST => post(&state, &fingerprint, request).await,
        _ => Err(ApiError::not_found()),
    }
}

fn get(state: &AppState, query: Option<&str>) -> Result<Response<Body>, ApiError> {
    get_at(state, query, crate::transfer_grants::now_ms())
}

fn get_at(
    state: &AppState,
    query: Option<&str>,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    let continuation = single_parameter(query.unwrap_or_default(), "continuation")
        .and_then(|value| SecretString::new(value).ok());
    let Some(continuation) = continuation else {
        return Ok(page::invalid_link());
    };
    let now = now?;
    let Ok(claims) =
        yard_session_contracts::verify(&state.yard_continuation_key, &continuation, now)
    else {
        return Ok(page::invalid_link());
    };
    Ok(page::login(
        claims.host_label(),
        continuation.expose_secret(),
        false,
    ))
}

async fn post(
    state: &AppState,
    fingerprint: &str,
    request: Request<Body>,
) -> Result<Response<Body>, ApiError> {
    post_at(
        state,
        fingerprint,
        request,
        crate::transfer_grants::now_ms(),
    )
    .await
}

async fn post_at(
    state: &AppState,
    fingerprint: &str,
    request: Request<Body>,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    let now = now?;
    yard_account::consume_rate_limit(state, fingerprint, "yard-login", now)?;
    let Some(body) = yard_account::form_body(request).await? else {
        return Ok(page::invalid_link());
    };
    let Some((continuation, login_key)) =
        login_form(&body).and_then(|(continuation, login_key)| {
            SecretString::new(continuation)
                .ok()
                .zip(SecretString::new(login_key).ok())
        })
    else {
        return Ok(page::invalid_link());
    };
    let Ok(claims) =
        yard_session_contracts::verify(&state.yard_continuation_key, &continuation, now)
    else {
        return Ok(page::invalid_link());
    };
    authenticate_and_redirect(state, &continuation, &login_key, &claims, now)
}

fn authenticate_and_redirect(
    state: &AppState,
    continuation: &SecretString,
    login_key: &SecretString,
    claims: &ContinuationClaims,
    now: u64,
) -> Result<Response<Body>, ApiError> {
    let Some(subject_id) = authenticate_subject(state, login_key, now)? else {
        return Ok(page::login(
            claims.host_label(),
            continuation.expose_secret(),
            true,
        ));
    };
    let admission =
        match state
            .repository
            .evaluate_yard_admission(claims.host_label(), &subject_id, now)
        {
            Ok(admission) => admission,
            Err(RepositoryError::NotFound) => return Ok(page::access_denied()),
            Err(_) => return Err(ApiError::internal()),
        };
    let code = crate::auth::generate_token(GeneratedSecretKind::YardExchangeCode);
    let durable = exchange_expiry(now).map(|expires_at_ms| NewYardContinuation {
        id: continuation_id(),
        continuation_hash: crate::auth::hash(continuation.expose_secret()),
        code_hash: crate::auth::hash(code.expose_secret()),
        yard_id: admission.yard_id,
        environment_id: admission.environment_id,
        host_label: claims.host_label().to_owned(),
        user_id: subject_id,
        return_path: claims.return_path().to_owned(),
        created_at_ms: now,
        expires_at_ms,
    });
    issue_durable_redirect(state, claims.host_label(), &code, durable)
}

fn authenticate_subject(
    state: &AppState,
    login_key: &SecretString,
    now: u64,
) -> Result<Option<String>, ApiError> {
    let raw = login_key.expose_secret();
    let result = if raw.starts_with("byg_") {
        if !yard_session_contracts::has_token_shape(raw, "byg_") {
            return Ok(None);
        }
        state
            .repository
            .authenticate_yard_guest_key(&crate::auth::hash(raw), now)
            .map(|subject| subject.id)
    } else {
        state
            .repository
            .authenticate_local_user_key(&crate::auth::hash(raw), now)
            .map(|user| user.id)
    };
    match result {
        Ok(subject_id) => Ok(Some(subject_id)),
        Err(RepositoryError::NotFound | RepositoryError::InvalidInput) => Ok(None),
        Err(_) => Err(ApiError::internal()),
    }
}

fn exchange_expiry(now: u64) -> Result<u64, ApiError> {
    now.checked_add(YARD_EXCHANGE_CODE_LIFETIME_MS)
        .ok_or_else(ApiError::internal)
}

fn issue_durable_redirect(
    state: &AppState,
    host_label: &str,
    code: &SecretString,
    durable: Result<NewYardContinuation, ApiError>,
) -> Result<Response<Body>, ApiError> {
    let durable = durable?;
    issue_redirect(
        state.repository.issue_yard_exchange_code(&durable),
        &state.web_yard_origin,
        host_label,
        code,
    )
}

fn issue_redirect(
    result: Result<(), RepositoryError>,
    origin: &str,
    host_label: &str,
    code: &SecretString,
) -> Result<Response<Body>, ApiError> {
    match result {
        Ok(()) => exchange_redirect(origin, host_label, code),
        Err(RepositoryError::Conflict) => Ok(page::invalid_link()),
        Err(RepositoryError::NotFound) => Ok(page::access_denied()),
        Err(_) => Err(ApiError::internal()),
    }
}

fn continuation_id() -> String {
    format!("yardcont_{}", uuid::Uuid::new_v4().simple())
}

fn exchange_redirect(
    origin: &str,
    host_label: &str,
    code: &SecretString,
) -> Result<Response<Body>, ApiError> {
    exchange_redirect_from_url(yard_session_contracts::yard_url(origin, host_label), code)
}

fn exchange_redirect_from_url(
    location: Result<String, ApiError>,
    code: &SecretString,
) -> Result<Response<Body>, ApiError> {
    let mut location = parse_location(&location?)?;
    location.set_path("/.blobyard/session/exchange");
    location
        .query_pairs_mut()
        .append_pair("code", code.expose_secret());
    crate::response::redirect(StatusCode::SEE_OTHER, location.as_str(), None)
}

fn parse_location(value: &str) -> Result<url::Url, ApiError> {
    url::Url::parse(value).map_err(|_error| ApiError::internal())
}

fn single_parameter(input: &str, name: &str) -> Option<String> {
    let mut values = url::form_urlencoded::parse(input.as_bytes());
    let value = values
        .next()
        .filter(|(candidate, _value)| candidate == name)?
        .1
        .into_owned();
    values.next().is_none().then_some(value)
}

fn login_form(input: &str) -> Option<(String, String)> {
    yard_account::exact_parameters(input, "continuation", "login_key")
}

#[cfg(test)]
#[path = "yard_login_tests.rs"]
mod tests;
