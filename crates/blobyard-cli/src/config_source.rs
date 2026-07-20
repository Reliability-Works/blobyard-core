/// Source of a resolved configuration value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigSource {
    /// Command-line flag.
    Flag,
    /// Process environment.
    Environment,
    /// Nearest project configuration.
    Project,
    /// Platform user configuration.
    User,
    /// Named user connection profile.
    Profile,
    /// Compiled production default.
    Default,
}

impl ConfigSource {
    /// Returns a stable diagnostic label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flag => "flag",
            Self::Environment => "environment",
            Self::Project => "project",
            Self::User => "user",
            Self::Profile => "profile",
            Self::Default => "default",
        }
    }
}
