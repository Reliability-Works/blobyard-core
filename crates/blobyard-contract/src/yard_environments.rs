use blobyard_core::Slug;

/// Persisted deployment-target class for one Yard environment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YardEnvironmentKind {
    /// The stable environment selected by the public alias.
    Production,
    /// A long-lived pre-production environment.
    Staging,
    /// A short-lived review environment.
    Preview,
}

impl YardEnvironmentKind {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Staging => "staging",
            Self::Preview => "preview",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "production" => Some(Self::Production),
            "staging" => Some(Self::Staging),
            "preview" => Some(Self::Preview),
            _ => None,
        }
    }
}

/// Persisted lifecycle state for one Yard environment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YardEnvironmentStatus {
    /// The environment accepts reads and future deployments.
    Active,
    /// The environment is unavailable and its name may be reused.
    Deleted,
}

impl YardEnvironmentStatus {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Deleted => "deleted",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "deleted" => Some(Self::Deleted),
            _ => None,
        }
    }
}

/// Durable metadata for one named Yard environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardEnvironmentRecord {
    /// Stable environment identifier.
    pub id: String,
    /// Parent Yard identifier.
    pub yard_id: String,
    /// Yard-unique environment name.
    pub name: Slug,
    /// Deployment-target class.
    pub kind: YardEnvironmentKind,
    /// Persisted lifecycle state.
    pub status: YardEnvironmentStatus,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Last change as Unix milliseconds.
    pub updated_at_ms: u64,
}

#[cfg(test)]
#[path = "yard_environments_tests.rs"]
mod tests;
