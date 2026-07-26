use std::error::Error;
use std::fmt::{Display, Formatter};

/// One application manifest validation failure at an exact field path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestError {
    path: String,
    message: String,
}

impl ManifestError {
    pub(crate) fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }

    /// Returns the canonical dotted field path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the validation explanation.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for ManifestError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

/// Every validation failure found in one application manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestErrors(Vec<ManifestError>);

impl ManifestErrors {
    pub(crate) const fn new(errors: Vec<ManifestError>) -> Self {
        Self(errors)
    }

    pub(crate) fn one(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self(vec![ManifestError::new(path, message)])
    }

    /// Returns every validation failure in deterministic field order.
    #[must_use]
    pub fn errors(&self) -> &[ManifestError] {
        &self.0
    }
}

impl Display for ManifestErrors {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str("\n")?;
            }
            Display::fmt(error, formatter)?;
        }
        Ok(())
    }
}

impl Error for ManifestErrors {}
