use crate::{api::AppState, error::ApiError, yard_session_contracts};
use axum::{
    body::{Body, to_bytes},
    http::{Request, header},
};

const MAXIMUM_FORM_BYTES: usize = 32 * 1_024;

pub(crate) fn require_identity_host(
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

pub(crate) fn require_identity_request(
    state: &AppState,
    request: &Request<Body>,
) -> Result<(), ApiError> {
    require_identity_host(state, request.headers())?;
    let mut origins = request.headers().get_all(header::ORIGIN).iter();
    let supplied = origins.next().and_then(|value| value.to_str().ok());
    if supplied == Some(state.public_origin.as_str()) && origins.next().is_none() {
        Ok(())
    } else {
        Err(ApiError::not_found())
    }
}

pub(crate) async fn form_body(request: Request<Body>) -> Result<Option<String>, ApiError> {
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
        return Ok(None);
    }
    let bytes = to_bytes(request.into_body(), MAXIMUM_FORM_BYTES)
        .await
        .map_err(|_error| ApiError::invalid_request())?;
    let body = std::str::from_utf8(&bytes).map_err(|_error| ApiError::invalid_request())?;
    Ok(Some(body.to_owned()))
}

pub(crate) fn exact_parameters(input: &str, first: &str, second: &str) -> Option<(String, String)> {
    let mut first_value = None;
    let mut second_value = None;
    for (name, value) in url::form_urlencoded::parse(input.as_bytes()) {
        let slot = if name == first {
            &mut first_value
        } else if name == second {
            &mut second_value
        } else {
            return None;
        };
        if slot.replace(value.into_owned()).is_some() {
            return None;
        }
    }
    first_value.zip(second_value)
}

pub(crate) fn consume_rate_limit(
    state: &AppState,
    fingerprint: &str,
    scope: &str,
    now: u64,
) -> Result<(), ApiError> {
    let rate_key = crate::auth::hash(&format!("{scope}\0{fingerprint}"));
    crate::inbox_rate::consume(
        state,
        &rate_key,
        blobyard_contract::YARD_LOGIN_RATE_WINDOW_MS,
        blobyard_contract::YARD_LOGIN_RATE_LIMIT,
        now,
    )
}
