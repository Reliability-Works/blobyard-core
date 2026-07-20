use crate::{ObjectSource, ProjectRecord, RepositoryError, ShareStatus, WorkspaceRecord};

/// One complete immutable object version prepared for an empty standalone installation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationObjectRecord {
    /// Stable destination version identifier.
    pub id: String,
    /// Parent destination project identifier.
    pub project_id: String,
    /// Logical object path.
    pub object_path: String,
    /// Preserved immutable version number.
    pub version: u64,
    /// Provider-independent destination storage key.
    pub storage_key: String,
    /// Verified byte size.
    pub size: u64,
    /// Verified lowercase SHA-256 checksum.
    pub checksum: String,
    /// Original object creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Original ingestion surface.
    pub source: ObjectSource,
    /// Optional normalized source repository.
    pub git_repository: Option<String>,
    /// Optional source commit identifier.
    pub git_commit: Option<String>,
    /// Optional source branch.
    pub git_branch: Option<String>,
    /// Original safe filename.
    pub filename: String,
    /// Original content type hint.
    pub content_type: String,
}

/// One redacted share policy prepared with a newly generated destination capability hash.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationShareRecord {
    /// Stable destination share identifier.
    pub id: String,
    /// Parent destination workspace identifier.
    pub workspace_id: String,
    /// Destination immutable object version.
    pub version_id: String,
    /// Lowercase SHA-256 digest of the newly generated capability.
    pub capability_hash: String,
    /// Preserved absolute expiry as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Preserved lifecycle state.
    pub status: ShareStatus,
    /// Preserved completed-download count.
    pub consumed_count: u64,
    /// Preserved optional maximum download count.
    pub maximum_downloads: Option<u64>,
    /// Preserved creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Preserved revocation time, when revoked.
    pub revoked_at_ms: Option<u64>,
}

/// One preserved retention policy, including its enabled state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationRetentionRecord {
    /// Parent destination project identifier.
    pub project_id: String,
    /// Number of newest matching versions preserved.
    pub keep_latest: u32,
    /// Optional logical-path glob.
    pub path_glob: Option<String>,
    /// Optional source-branch glob.
    pub branch_glob: Option<String>,
    /// Whether enforcement is enabled.
    pub enabled: bool,
    /// Preserved creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Preserved last update time as Unix milliseconds.
    pub updated_at_ms: u64,
}

/// Complete validated metadata for one hosted-to-standalone migration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationSnapshot {
    /// Destination workspaces in stable slug order.
    pub workspaces: Vec<WorkspaceRecord>,
    /// Destination projects in stable workspace and slug order.
    pub projects: Vec<ProjectRecord>,
    /// Complete object versions in stable URI order.
    pub objects: Vec<MigrationObjectRecord>,
    /// Redacted share policies in stable creation order.
    pub shares: Vec<MigrationShareRecord>,
    /// Retention policies in stable project order.
    pub retention: Vec<MigrationRetentionRecord>,
}

/// Atomic metadata import required by hosted-to-standalone migration tooling.
pub trait MigrationRepository: Send + Sync {
    /// Imports one complete snapshot into an otherwise empty metadata repository.
    ///
    /// # Errors
    ///
    /// Returns validation, conflict, or persistence failures without committing a partial import.
    fn import_migration(&self, snapshot: &MigrationSnapshot) -> Result<(), RepositoryError>;
}
