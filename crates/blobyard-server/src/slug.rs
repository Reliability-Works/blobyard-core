use crate::error::ApiError;
use blobyard_core::Slug;

pub(crate) fn parse(value: String) -> Result<Slug, ApiError> {
    Slug::new(value).map_err(|_error| ApiError::invalid_request())
}

pub(crate) fn validate_name(value: &str) -> Result<(), ApiError> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        Err(ApiError::invalid_request())
    } else {
        Ok(())
    }
}

pub(crate) fn from_name(name: &str) -> Option<Slug> {
    let mut value = String::with_capacity(name.len().min(63));
    let mut separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !value.is_empty() && value.len() < 63 {
                value.push('-');
            }
            separator = false;
            if value.len() < 63 {
                value.push(character.to_ascii_lowercase());
            }
        } else if matches!(character, ' ' | '-' | '_') {
            separator = true;
        } else {
            return None;
        }
    }
    Slug::new(value).ok()
}

#[cfg(test)]
#[path = "slug_tests.rs"]
mod tests;
