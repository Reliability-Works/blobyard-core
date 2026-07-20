use crate::NewAuditEvent;
use blobyard_core::Slug;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Shared persisted lifecycle for an active or revoked public capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevocableStatus {
    /// The capability can still be used.
    Active,
    /// An authenticated operator revoked the capability.
    Revoked,
}

impl RevocableStatus {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "revoked" => Some(Self::Revoked),
            _ => None,
        }
    }
}

/// Stable repository failure classes used by every metadata adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryError {
    /// The requested record does not exist.
    NotFound,
    /// A uniqueness or state transition constraint failed.
    Conflict,
    /// Input was invalid for the repository contract.
    InvalidInput,
    /// The schema is newer than this runtime supports.
    SchemaTooNew,
    /// The metadata provider failed.
    Unavailable,
}

impl Display for RepositoryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "metadata record not found",
            Self::Conflict => "metadata conflict",
            Self::InvalidInput => "invalid metadata input",
            Self::SchemaTooNew => "metadata schema is newer than this runtime",
            Self::Unavailable => "metadata repository unavailable",
        })
    }
}

impl Error for RepositoryError {}

/// A local workspace namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceRecord {
    /// Stable workspace identifier.
    pub id: String,
    /// User-facing workspace name.
    pub name: String,
    /// URI namespace.
    pub slug: Slug,
}

/// A project within a workspace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRecord {
    /// Stable project identifier.
    pub id: String,
    /// Parent workspace identifier.
    pub workspace_id: String,
    /// User-facing project name.
    pub name: String,
    /// URI namespace.
    pub slug: Slug,
}

/// Lifecycle state for an object-version upload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadState {
    /// Bytes have not been committed yet.
    Pending,
    /// Bytes and integrity metadata were committed atomically.
    Complete,
    /// The reservation was abandoned.
    Aborted,
}

impl UploadState {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Complete => "complete",
            Self::Aborted => "aborted",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "complete" => Some(Self::Complete),
            "aborted" => Some(Self::Aborted),
            _ => None,
        }
    }
}

/// Input used to reserve one immutable object version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewObjectVersion {
    /// Stable version identifier.
    pub id: String,
    /// Parent project identifier.
    pub project_id: String,
    /// Logical object path.
    pub object_path: String,
    /// Monotonically increasing version number for the object path.
    pub version: u64,
    /// Provider-independent storage key.
    pub storage_key: String,
    /// Authenticated ingestion surface that created this version.
    pub source: ObjectSource,
    /// Optional normalized source repository.
    pub git_repository: Option<String>,
    /// Optional source commit identifier.
    pub git_commit: Option<String>,
    /// Optional source branch.
    pub git_branch: Option<String>,
}

/// Authenticated ingestion surface that created an immutable object version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectSource {
    /// GitHub Actions or another verified CI trust.
    Ci,
    /// Native CLI, SDK, MCP, or a local API token.
    Cli,
    /// Public upload inbox.
    Inbox,
    /// Preview publication.
    Preview,
    /// Authenticated web dashboard.
    Web,
}

impl ObjectSource {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ci => "ci",
            Self::Cli => "cli",
            Self::Inbox => "inbox",
            Self::Preview => "preview",
            Self::Web => "web",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ci" => Some(Self::Ci),
            "cli" => Some(Self::Cli),
            "inbox" => Some(Self::Inbox),
            "preview" => Some(Self::Preview),
            "web" => Some(Self::Web),
            _ => None,
        }
    }
}

/// Persisted immutable object-version metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectVersionRecord {
    /// Stable version identifier.
    pub id: String,
    /// Parent project identifier.
    pub project_id: String,
    /// Logical object path.
    pub object_path: String,
    /// Monotonically increasing version number for the object path.
    pub version: u64,
    /// Provider-independent storage key.
    pub storage_key: String,
    /// Current upload state.
    pub state: UploadState,
    /// Committed byte size, when complete.
    pub size: Option<u64>,
    /// Lowercase SHA-256 checksum, when complete.
    pub checksum: Option<String>,
    /// Durable creation timestamp as Unix milliseconds.
    pub created_at_ms: u64,
    /// Authenticated ingestion surface that created this version.
    pub source: ObjectSource,
    /// Optional normalized source repository.
    pub git_repository: Option<String>,
    /// Optional source commit identifier.
    pub git_commit: Option<String>,
    /// Optional source branch provenance.
    pub git_branch: Option<String>,
}

/// Durable metadata operations required by the standalone runtime core.
pub trait MetadataRepository: Send + Sync {
    /// Returns the applied schema version.
    ///
    /// # Errors
    ///
    /// Returns a stable repository failure when the schema cannot be read.
    fn schema_version(&self) -> Result<u32, RepositoryError>;

    /// Creates a workspace, rejecting duplicate identifiers or slugs.
    ///
    /// # Errors
    ///
    /// Returns a validation, conflict, or provider failure.
    fn create_workspace(&self, workspace: &WorkspaceRecord) -> Result<(), RepositoryError>;

    /// Lists workspaces in stable slug order.
    ///
    /// # Errors
    ///
    /// Returns a stable repository failure when records cannot be read.
    fn list_workspaces(&self) -> Result<Vec<WorkspaceRecord>, RepositoryError>;

    /// Finds one workspace by slug.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or a provider failure.
    fn workspace_by_slug(&self, slug: &Slug) -> Result<WorkspaceRecord, RepositoryError>;

    /// Replaces a workspace's display name and URI slug without changing its stable identifier.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or a provider failure.
    fn rename_workspace(
        &self,
        workspace: &WorkspaceRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError>;

    /// Creates a project, rejecting missing workspaces and duplicate slugs.
    ///
    /// # Errors
    ///
    /// Returns a validation, conflict, or provider failure.
    fn create_project(&self, project: &ProjectRecord) -> Result<(), RepositoryError>;

    /// Lists projects for a workspace in stable slug order.
    ///
    /// # Errors
    ///
    /// Returns a validation or provider failure.
    fn list_projects(&self, workspace_id: &str) -> Result<Vec<ProjectRecord>, RepositoryError>;

    /// Finds one project by workspace and slug.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or a provider failure.
    fn project_by_slug(
        &self,
        workspace_id: &str,
        slug: &Slug,
    ) -> Result<ProjectRecord, RepositoryError>;

    /// Reserves an immutable object version in the pending state.
    ///
    /// # Errors
    ///
    /// Returns a validation, conflict, or provider failure.
    fn reserve_object_version(&self, version: &NewObjectVersion) -> Result<(), RepositoryError>;

    /// Commits integrity metadata exactly once for a pending version.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failure.
    fn complete_object_version(
        &self,
        id: &str,
        size: u64,
        checksum: &str,
    ) -> Result<(), RepositoryError>;

    /// Aborts a pending version exactly once.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failure.
    fn abort_object_version(&self, id: &str) -> Result<(), RepositoryError>;

    /// Reads one object version by stable identifier.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or provider failure.
    fn object_version(&self, id: &str) -> Result<ObjectVersionRecord, RepositoryError>;
}

/// Read-only metadata inventory used by standalone integrity and recovery tooling.
///
/// This contract stays separate from [`MetadataRepository`] so request-path repositories do not
/// gain operator-only methods.
pub trait MetadataRepositoryInventory: Send + Sync {
    /// Lists every object-version record in stable storage-key order.
    ///
    /// # Errors
    ///
    /// Returns a stable repository failure when any record cannot be read or decoded.
    fn list_object_versions(&self) -> Result<Vec<ObjectVersionRecord>, RepositoryError>;
}
