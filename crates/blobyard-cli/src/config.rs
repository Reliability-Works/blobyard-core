#![allow(
    clippy::redundant_pub_crate,
    reason = "runner siblings consume the Yard-name validator through this private config module"
)]

use crate::GlobalArgs;
use blobyard_api_client::ApiClientConfig;
use blobyard_core::{BlobyardError, ErrorCode, SecretString, Slug, WebYardOrigin};
use directories::ProjectDirs;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[path = "config_file.rs"]
mod config_file;
#[path = "config_profile.rs"]
mod config_profile;
#[path = "config_profile_write.rs"]
mod config_profile_write;
#[path = "config_resolution.rs"]
mod config_resolution;
#[path = "config_source.rs"]
mod config_source;
#[path = "config_values.rs"]
mod config_values;
#[path = "config_yards.rs"]
mod config_yards;

use config_file::{ConfigLayer, discover_project_file, read_layer};
pub(crate) use config_profile::DEFAULT_PROFILE;
pub(crate) use config_profile_write::{ensure_new_profile, write_self_hosted_profile};
use config_resolution::resolve_connection;
pub use config_source::ConfigSource;
pub use config_yards::YardConfig;
pub(crate) use config_yards::validate_yard_name;

const PROJECT_CONFIG_NAME: &str = ".blobyard.toml";
const MAX_CONFIG_BYTES: u64 = 65_536;

/// Filesystem locations used during configuration discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigPaths {
    cwd: PathBuf,
    user_config: PathBuf,
}

impl ConfigPaths {
    /// Creates explicit paths, primarily for deterministic callers and tests.
    #[must_use]
    pub fn new(cwd: impl Into<PathBuf>, user_config: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            user_config: user_config.into(),
        }
    }

    /// Discovers paths for the running user.
    ///
    /// # Errors
    ///
    /// Returns a safe internal error when the platform has no config directory
    /// or the current directory is unavailable.
    pub fn system() -> Result<Self, BlobyardError> {
        let cwd = std::env::current_dir().ok();
        let config_dir = ProjectDirs::from("com", "Blobyard", "Blobyard")
            .map(|dirs| dirs.config_dir().to_owned());
        Self::from_system_parts(cwd, config_dir)
    }

    /// Returns the current working directory used for upward discovery.
    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Returns the platform user config file.
    #[must_use]
    pub fn user_config(&self) -> &Path {
        &self.user_config
    }

    /// Returns the explicit credential-file fallback location.
    #[must_use]
    pub fn credentials_file(&self, profile: &Slug) -> PathBuf {
        let name = if profile.as_str() == config_profile::DEFAULT_PROFILE {
            "credentials".to_owned()
        } else {
            format!("credentials.{}", profile.as_str())
        };
        self.user_config.with_file_name(name)
    }

    fn from_system_parts(
        cwd: Option<PathBuf>,
        config_dir: Option<PathBuf>,
    ) -> Result<Self, BlobyardError> {
        let cwd = cwd.ok_or_else(local_io_error)?;
        let config_dir =
            config_dir.ok_or_else(|| BlobyardError::from_code(ErrorCode::InternalError))?;
        Ok(Self::new(cwd, config_dir.join("config.toml")))
    }
}

/// Read-only environment seam used by configuration discovery.
pub trait Environment: Send + Sync {
    /// Returns a Unicode environment value, when present.
    fn get(&self, key: &str) -> Option<String>;
}

/// Environment backed by the running process.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessEnvironment;

impl Environment for ProcessEnvironment {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// Fully validated effective CLI configuration.
#[derive(Clone, Debug)]
pub struct ResolvedConfig {
    profile: Slug,
    profile_source: ConfigSource,
    api: ApiClientConfig,
    api_source: ConfigSource,
    web_yard_origin: WebYardOrigin,
    workspace: Option<Slug>,
    workspace_source: Option<ConfigSource>,
    project: Option<Slug>,
    project_source: Option<ConfigSource>,
    environment_token: Option<SecretString>,
    project_file: Option<PathBuf>,
    yards: BTreeMap<String, YardConfig>,
    paths: ConfigPaths,
}

impl ResolvedConfig {
    /// Returns the selected connection profile.
    #[must_use]
    pub const fn profile(&self) -> &Slug {
        &self.profile
    }

    /// Returns the profile selection source.
    #[must_use]
    pub const fn profile_source(&self) -> ConfigSource {
        self.profile_source
    }

    /// Returns the API configuration.
    #[must_use]
    pub const fn api(&self) -> &ApiClientConfig {
        &self.api
    }

    /// Returns the API configuration source.
    #[must_use]
    pub const fn api_source(&self) -> ConfigSource {
        self.api_source
    }

    /// Returns the trusted root origin used by public Web Yard subdomains.
    #[must_use]
    pub const fn web_yard_origin(&self) -> &WebYardOrigin {
        &self.web_yard_origin
    }

    /// Returns the selected workspace.
    #[must_use]
    pub const fn workspace(&self) -> Option<&Slug> {
        self.workspace.as_ref()
    }

    /// Returns the workspace selection source.
    #[must_use]
    pub const fn workspace_source(&self) -> Option<ConfigSource> {
        self.workspace_source
    }

    /// Returns the selected project.
    #[must_use]
    pub const fn project(&self) -> Option<&Slug> {
        self.project.as_ref()
    }

    /// Returns the project selection source.
    #[must_use]
    pub const fn project_source(&self) -> Option<ConfigSource> {
        self.project_source
    }

    /// Returns the temporary environment bearer, which suppresses token-store lookup.
    #[must_use]
    pub const fn environment_token(&self) -> Option<&SecretString> {
        self.environment_token.as_ref()
    }

    /// Returns the redaction-safe credential source label.
    #[must_use]
    pub const fn token_source(&self) -> &'static str {
        if self.environment_token.is_some() {
            "environment"
        } else {
            "credential_store"
        }
    }

    pub(crate) fn with_scope(
        &self,
        workspace: Option<String>,
        project: Option<String>,
    ) -> Result<Self, BlobyardError> {
        let mut scoped = self.clone();
        if let Some(workspace) = workspace {
            scoped.workspace = Some(valid_mcp_slug(workspace, "workspace")?);
            scoped.workspace_source = Some(ConfigSource::Flag);
        }
        if let Some(project) = project {
            scoped.project = Some(valid_mcp_slug(project, "project")?);
            scoped.project_source = Some(ConfigSource::Flag);
        }
        Ok(scoped)
    }

    /// Returns the discovered project config file.
    #[must_use]
    pub fn project_file(&self) -> Option<&Path> {
        self.project_file.as_deref()
    }

    /// Returns named Web Yards from the nearest project configuration.
    #[must_use]
    pub const fn yards(&self) -> &BTreeMap<String, YardConfig> {
        &self.yards
    }

    /// Returns filesystem discovery paths.
    #[must_use]
    pub const fn paths(&self) -> &ConfigPaths {
        &self.paths
    }
}

fn valid_mcp_slug(value: String, name: &str) -> Result<Slug, BlobyardError> {
    Slug::new(value).map_err(|_error| {
        BlobyardError::new(
            ErrorCode::InvalidRequest,
            format!("The {name} slug is not valid."),
        )
    })
}

/// Loads and validates configuration with the documented precedence.
pub struct ConfigLoader<'a> {
    paths: ConfigPaths,
    environment: &'a dyn Environment,
}

impl<'a> ConfigLoader<'a> {
    /// Creates a loader over explicit seams.
    #[must_use]
    pub const fn new(paths: ConfigPaths, environment: &'a dyn Environment) -> Self {
        Self { paths, environment }
    }

    /// Resolves command flags, environment, project, user, and defaults.
    ///
    /// # Errors
    ///
    /// Returns `INVALID_REQUEST` for malformed files, unsafe endpoints, or
    /// invalid slugs. I/O failures return a safe local error.
    pub fn load(&self, flags: &GlobalArgs) -> Result<ResolvedConfig, BlobyardError> {
        let project_file = discover_project_file(self.paths.cwd());
        let project = read_layer(project_file.as_deref())?;
        let user = read_layer(Some(self.paths.user_config()))?;
        let connection =
            resolve_connection(flags, self.environment, project.as_ref(), user.as_ref())?;
        let environment_token = self
            .environment
            .get("BLOBYARD_TOKEN")
            .map(SecretString::new)
            .transpose()?;
        let yards = config_yards::resolve_yards(project_file.as_deref(), project.as_ref())?;
        Ok(ResolvedConfig {
            profile: connection.profile,
            profile_source: connection.profile_source,
            api: connection.api,
            api_source: connection.api_source,
            web_yard_origin: connection.web_yard_origin,
            workspace: connection.workspace,
            workspace_source: connection.workspace_source,
            project: connection.project,
            project_source: connection.project_source,
            environment_token,
            project_file,
            yards,
            paths: self.paths.clone(),
        })
    }
}

fn invalid_config() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InvalidRequest,
        "A Blobyard config file isn't valid. Fix or remove it and try again.",
    )
}

fn local_io_error() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InternalError,
        "Blobyard couldn't read local configuration. Check file permissions and try again.",
    )
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
