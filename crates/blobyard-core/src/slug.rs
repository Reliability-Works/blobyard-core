use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

const MAX_SLUG_BYTES: usize = 63;

/// A validated workspace or project slug.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct Slug(String);

impl Slug {
    /// Validates a slug.
    ///
    /// # Errors
    ///
    /// Returns [`SlugError`] when the value is empty, overlong, has unsafe
    /// edge characters, or contains characters outside ASCII letters,
    /// digits, hyphens, and underscores.
    pub fn new(value: impl Into<String>) -> Result<Self, SlugError> {
        let value = value.into();
        let valid_length = !value.is_empty() && value.len() <= MAX_SLUG_BYTES;
        let valid_edges =
            value.starts_with(char::is_alphanumeric) && value.ends_with(char::is_alphanumeric);
        let valid_chars = value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
        if valid_length && valid_edges && valid_chars {
            Ok(Self(value))
        } else {
            Err(SlugError)
        }
    }

    /// Returns the validated slug text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Slug {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Slug {
    type Err = SlugError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for Slug {
    type Error = SlugError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Slug> for String {
    fn from(value: Slug) -> Self {
        value.0
    }
}

/// A slug failed validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SlugError;

impl Display for SlugError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("slug must use 1-63 ASCII letters, digits, hyphens, or underscores")
    }
}

impl Error for SlugError {}
