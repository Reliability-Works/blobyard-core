use crate::{
    api::AppState,
    error::ApiError,
    inboxes::PeerFingerprint,
    yard_session_contracts::{self, ContinuationClaims},
};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::State,
    http::{Method, Request, Response, StatusCode, header},
    routing::any,
};
use blobyard_contract::{
    NewYardContinuation, RepositoryError, YARD_EXCHANGE_CODE_LIFETIME_MS, YARD_LOGIN_RATE_LIMIT,
    YARD_LOGIN_RATE_WINDOW_MS,
};
use blobyard_core::{GeneratedSecretKind, SecretString};

const MAXIMUM_FORM_BYTES: usize = 32 * 1_024;

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
    require_identity_host(&state, request.headers())?;
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
        return page::invalid_link();
    };
    let now = now?;
    let Ok(claims) =
        yard_session_contracts::verify(&state.yard_continuation_key, &continuation, now)
    else {
        return page::invalid_link();
    };
    page::login(claims.host_label(), continuation.expose_secret(), false)
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
    let rate_key = crate::auth::hash(&format!("yard-login\0{fingerprint}"));
    crate::inbox_rate::consume(
        state,
        &rate_key,
        YARD_LOGIN_RATE_WINDOW_MS,
        YARD_LOGIN_RATE_LIMIT,
        now,
    )?;
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if content_type
        .split(';')
        .next()
        .is_none_or(|value| value.trim() != "application/x-www-form-urlencoded")
    {
        return page::invalid_link();
    }
    let body = to_bytes(request.into_body(), MAXIMUM_FORM_BYTES)
        .await
        .map_err(|_error| ApiError::invalid_request())?;
    let body = std::str::from_utf8(&body).map_err(|_error| ApiError::invalid_request())?;
    let Some((continuation, login_key)) = login_form(body).and_then(|(continuation, login_key)| {
        SecretString::new(continuation)
            .ok()
            .zip(SecretString::new(login_key).ok())
    }) else {
        return page::invalid_link();
    };
    let Ok(claims) =
        yard_session_contracts::verify(&state.yard_continuation_key, &continuation, now)
    else {
        return page::invalid_link();
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
    let user = match state
        .repository
        .authenticate_local_user_key(&crate::auth::hash(login_key.expose_secret()), now)
    {
        Ok(user) => user,
        Err(RepositoryError::NotFound | RepositoryError::InvalidInput) => {
            return page::login(claims.host_label(), continuation.expose_secret(), true);
        }
        Err(_) => return Err(ApiError::internal()),
    };
    let admission =
        match state
            .repository
            .evaluate_yard_admission(claims.host_label(), &user.id, now)
        {
            Ok(admission) => admission,
            Err(RepositoryError::NotFound) => return page::access_denied(),
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
        user_id: user.id,
        return_path: claims.return_path().to_owned(),
        created_at_ms: now,
        expires_at_ms,
    });
    issue_durable_redirect(state, claims.host_label(), &code, durable)
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
        Err(RepositoryError::Conflict) => page::invalid_link(),
        Err(RepositoryError::NotFound) => page::access_denied(),
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
    redirect(StatusCode::SEE_OTHER, location.as_str())
}

fn parse_location(value: &str) -> Result<url::Url, ApiError> {
    url::Url::parse(value).map_err(|_error| ApiError::internal())
}

fn redirect(status: StatusCode, location: &str) -> Result<Response<Body>, ApiError> {
    ApiError::internal_result(
        Response::builder()
            .status(status)
            .header(header::LOCATION, location)
            .header(header::CACHE_CONTROL, "no-store")
            .header(header::REFERRER_POLICY, "no-referrer")
            .body(Body::empty()),
    )
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
    let mut continuation = None;
    let mut login_key = None;
    for (name, value) in url::form_urlencoded::parse(input.as_bytes()) {
        let slot = match name.as_ref() {
            "continuation" => &mut continuation,
            "login_key" => &mut login_key,
            _ => return None,
        };
        if slot.replace(value.into_owned()).is_some() {
            return None;
        }
    }
    continuation.zip(login_key)
}

fn require_identity_host(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<(), ApiError> {
    let authority = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let expected = yard_session_contracts::identity_authority(&state.public_origin);
    if authority == expected.as_deref() {
        Ok(())
    } else {
        Err(ApiError::not_found())
    }
}

#[cfg(test)]
#[path = "yard_login_tests.rs"]
mod tests;
