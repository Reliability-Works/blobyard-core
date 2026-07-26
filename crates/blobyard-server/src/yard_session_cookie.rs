use axum::http::{HeaderMap, HeaderValue, header};
use blobyard_contract::YARD_SESSION_COOKIE_NAME;
use blobyard_core::SecretString;

pub(crate) fn read(headers: &HeaderMap) -> Option<SecretString> {
    let mut matching = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .filter_map(|cookie| cookie.trim().split_once('='))
        .filter_map(|(name, value)| (name == YARD_SESSION_COOKIE_NAME).then_some(value));
    let value = matching.next()?;
    if matching.next().is_some() || !crate::yard_session_contracts::has_token_shape(value, "byys_")
    {
        return None;
    }
    SecretString::new(value.to_owned()).ok()
}

pub(crate) fn set_header(token: &SecretString) -> Result<HeaderValue, ()> {
    HeaderValue::from_str(&format!(
        "{YARD_SESSION_COOKIE_NAME}={}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=43200",
        token.expose_secret()
    ))
    .map_err(|_error| ())
}

pub(crate) const fn clear_header() -> HeaderValue {
    HeaderValue::from_static(
        "__Host-blobyard-yard-session=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0",
    )
}

#[cfg(test)]
#[path = "yard_session_cookie_tests.rs"]
mod tests;
