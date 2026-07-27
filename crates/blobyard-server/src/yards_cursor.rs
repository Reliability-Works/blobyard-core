use crate::error::ApiError;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::de::DeserializeOwned;

pub(super) const MAXIMUM_CURSOR_LENGTH: usize = 1_024;

pub(super) fn encode(value: &serde_json::Value) -> String {
    URL_SAFE_NO_PAD.encode(value.to_string())
}

pub(super) fn decode<T: DeserializeOwned>(value: Option<&str>) -> Result<Option<T>, ApiError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if !(1..=MAXIMUM_CURSOR_LENGTH).contains(&value.len()) {
        return Err(ApiError::invalid_request());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_error| ApiError::invalid_request())?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_error| ApiError::invalid_request())
}
