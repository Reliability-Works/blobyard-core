use crate::{StorageConfiguration, auth};
use blobyard_core::{GeneratedSecretKind, SecretString};
use serde::Serialize;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "hosted_migration_export.rs"]
mod export;
#[path = "hosted_migration_import.rs"]
mod import;
#[path = "hosted_migration_projection.rs"]
mod projection;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(2);
const DEFAULT_POLL_LIMIT: u32 = 900;

/// Inputs for one authenticated Cloud-to-standalone migration.
#[derive(Clone, Debug)]
pub struct HostedMigrationOptions {
    /// Blob Yard Cloud API origin, or a loopback-compatible fixture origin.
    pub source_url: String,
    /// New standalone installation directory.
    pub data_directory: PathBuf,
    /// Public standalone origin used for newly minted share URLs.
    pub public_url: String,
    /// Optional workspace slugs. An empty list selects every active exported workspace.
    pub workspace_slugs: Vec<String>,
    /// Destination storage backend.
    pub storage: StorageConfiguration,
    #[doc(hidden)]
    pub poll_interval: Duration,
    #[doc(hidden)]
    pub poll_limit: u32,
}

impl HostedMigrationOptions {
    /// Creates migration options with bounded polling defaults.
    #[must_use]
    pub const fn new(
        source_url: String,
        data_directory: PathBuf,
        public_url: String,
        workspace_slugs: Vec<String>,
        storage: StorageConfiguration,
    ) -> Self {
        Self {
            source_url,
            data_directory,
            public_url,
            workspace_slugs,
            storage,
            poll_interval: DEFAULT_POLL_INTERVAL,
            poll_limit: DEFAULT_POLL_LIMIT,
        }
    }
}

/// Redaction-safe hosted migration failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostedMigrationError {
    /// Command configuration or selected workspace input was invalid.
    InvalidInput,
    /// The source API rejected or could not complete the export.
    SourceApi,
    /// A signed artifact or object response was unsafe or unavailable.
    SourceDownload,
    /// Export metadata was malformed, inconsistent, unsupported, or incomplete.
    InvalidExport,
    /// Exported bytes did not match their immutable metadata.
    Integrity,
    /// The destination data directory already exists.
    DestinationExists,
    /// The destination storage namespace was not empty.
    StorageNotEmpty,
    /// Destination metadata could not be imported atomically.
    Metadata,
    /// Destination object storage failed.
    Storage,
    /// Imported destination state could not be activated or cleaned up safely.
    Persistence,
}

impl Display for HostedMigrationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInput => "hosted migration input is invalid",
            Self::SourceApi => "hosted migration source API is unavailable",
            Self::SourceDownload => "hosted migration source download is unavailable or unsafe",
            Self::InvalidExport => "hosted migration export is malformed or inconsistent",
            Self::Integrity => "hosted migration bytes do not match immutable metadata",
            Self::DestinationExists => "hosted migration destination already exists",
            Self::StorageNotEmpty => "hosted migration requires an empty storage namespace",
            Self::Metadata => "hosted migration metadata import failed",
            Self::Storage => "hosted migration object storage failed",
            Self::Persistence => "hosted migration could not activate or clean up the destination",
        })
    }
}

impl std::error::Error for HostedMigrationError {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MigrationReport<'a> {
    format: &'static str,
    workspace_count: usize,
    project_count: usize,
    object_version_count: usize,
    share_policy_count: usize,
    retention_policy_count: usize,
    bootstrap_token: &'a str,
    share_urls: Vec<&'a str>,
}

/// Migrates selected active Cloud workspaces into one absent standalone installation.
///
/// The source token is used only for export and immutable download grants. Every artifact and
/// object is verified before destination metadata is activated. Raw destination bootstrap and
/// replacement share capabilities are returned once in the JSON report.
///
/// # Errors
///
/// Fails closed for unsafe input, source drift, integrity disagreement, occupied destinations,
/// provider failures, or incomplete cleanup.
pub async fn migrate_from_hosted(
    options: &HostedMigrationOptions,
    source_token: SecretString,
) -> Result<String, HostedMigrationError> {
    validate_options(options)?;
    let exported = export::download(options, source_token).await?;
    let mut generate = auth::generate_token;
    let prepared =
        projection::prepare(&exported.datasets, &options.workspace_slugs, &mut generate)?;
    let objects = export::download_objects(&exported, &prepared.source_objects).await?;
    let bootstrap = auth::generate_token(GeneratedSecretKind::BootstrapToken);
    import::activate(options, &prepared, &objects, &bootstrap)?;
    encode_report(&prepared, &bootstrap, &options.public_url)
}

fn validate_options(options: &HostedMigrationOptions) -> Result<(), HostedMigrationError> {
    if options.poll_limit == 0
        || options.poll_interval.is_zero()
        || options.workspace_slugs.iter().any(String::is_empty)
        || options.data_directory.as_os_str().is_empty()
    {
        return Err(HostedMigrationError::InvalidInput);
    }
    crate::normalize_origin(&options.public_url)
        .map(|_origin| ())
        .map_err(|_error| HostedMigrationError::InvalidInput)
}

fn encode_report(
    prepared: &projection::PreparedMigration,
    bootstrap: &SecretString,
    public_url: &str,
) -> Result<String, HostedMigrationError> {
    let share_urls = prepared
        .share_capabilities
        .iter()
        .map(|capability| format!("{public_url}/s/{}", capability.expose_secret()))
        .collect::<Vec<_>>();
    let report = MigrationReport {
        format: "Blob Yard hosted migration v1",
        workspace_count: prepared.snapshot.workspaces.len(),
        project_count: prepared.snapshot.projects.len(),
        object_version_count: prepared.snapshot.objects.len(),
        share_policy_count: prepared.snapshot.shares.len(),
        retention_policy_count: prepared.snapshot.retention.len(),
        bootstrap_token: bootstrap.expose_secret(),
        share_urls: share_urls.iter().map(String::as_str).collect(),
    };
    serde_json::to_string_pretty(&report).map_err(|_error| HostedMigrationError::Persistence)
}

fn destination_parent(path: &Path) -> Result<&Path, HostedMigrationError> {
    match path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Ok(Path::new(".")),
        Some(parent) => Ok(parent),
        None => Err(HostedMigrationError::InvalidInput),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> HostedMigrationOptions {
        HostedMigrationOptions::new(
            "https://api.blobyard.com".to_owned(),
            PathBuf::from("installation"),
            "http://127.0.0.1:8787".to_owned(),
            vec!["workspace".to_owned()],
            StorageConfiguration::Filesystem,
        )
    }

    #[test]
    fn errors_have_stable_redacted_messages() {
        let cases = [
            (
                HostedMigrationError::InvalidInput,
                "hosted migration input is invalid",
            ),
            (
                HostedMigrationError::SourceApi,
                "hosted migration source API is unavailable",
            ),
            (
                HostedMigrationError::SourceDownload,
                "hosted migration source download is unavailable or unsafe",
            ),
            (
                HostedMigrationError::InvalidExport,
                "hosted migration export is malformed or inconsistent",
            ),
            (
                HostedMigrationError::Integrity,
                "hosted migration bytes do not match immutable metadata",
            ),
            (
                HostedMigrationError::DestinationExists,
                "hosted migration destination already exists",
            ),
            (
                HostedMigrationError::StorageNotEmpty,
                "hosted migration requires an empty storage namespace",
            ),
            (
                HostedMigrationError::Metadata,
                "hosted migration metadata import failed",
            ),
            (
                HostedMigrationError::Storage,
                "hosted migration object storage failed",
            ),
            (
                HostedMigrationError::Persistence,
                "hosted migration could not activate or clean up the destination",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn option_validation_rejects_each_unsafe_boundary() {
        let mutations: [fn(&mut HostedMigrationOptions); 5] = [
            |value| value.poll_limit = 0,
            |value| value.poll_interval = Duration::ZERO,
            |value| value.workspace_slugs.push(String::new()),
            |value| value.data_directory = PathBuf::new(),
            |value| value.public_url = "https://user@example.com".to_owned(),
        ];
        for mutate in mutations {
            let mut candidate = options();
            mutate(&mut candidate);
            assert_eq!(
                validate_options(&candidate),
                Err(HostedMigrationError::InvalidInput)
            );
        }
        assert!(validate_options(&options()).is_ok());
    }

    #[test]
    fn destination_parent_accepts_relative_and_nested_paths() {
        assert_eq!(
            destination_parent(Path::new("installation")),
            Ok(Path::new("."))
        );
        assert_eq!(
            destination_parent(Path::new("root/installation")),
            Ok(Path::new("root"))
        );
        assert_eq!(
            destination_parent(Path::new("/")),
            Err(HostedMigrationError::InvalidInput)
        );
    }
}
