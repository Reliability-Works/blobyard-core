use crate::StoredObjectRecord;
use blobyard_core::Slug;

/// Maximum portable relative path length for one immutable Yard file.
pub const MAXIMUM_YARD_PATH_BYTES: usize = 1_024;

/// Returns whether a Yard file path is portable and normalized.
#[must_use]
pub fn is_valid_yard_path(value: &str) -> bool {
    crate::is_valid_relative_path(value, MAXIMUM_YARD_PATH_BYTES)
}

/// Returns whether a normalized public request path is safe to resolve.
#[must_use]
pub fn is_valid_yard_request_path(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    let trimmed = value.strip_suffix('/').unwrap_or(value);
    !trimmed.is_empty() && is_valid_yard_path(trimmed)
}

/// Persisted lifecycle state for one named Web Yard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebYardStatus {
    /// The stable alias may serve its current deploy.
    Active,
    /// Delivery is administratively suspended.
    Suspended,
    /// The Yard and every immutable deploy are unavailable.
    Deleted,
}

impl WebYardStatus {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Deleted => "deleted",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "suspended" => Some(Self::Suspended),
            "deleted" => Some(Self::Deleted),
            _ => None,
        }
    }
}

/// Persisted lifecycle state for one immutable Web Yard deploy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YardDeployStatus {
    /// The client is uploading the reserved manifest.
    Uploading,
    /// The manifest is being validated and snapshotted.
    Finalising,
    /// The stable Yard alias currently selects this deploy.
    Live,
    /// The deploy did not complete successfully.
    Failed,
    /// Another retained deploy owns the stable alias.
    Superseded,
    /// Retention or Yard deletion made the deploy unavailable.
    Pruned,
}

impl YardDeployStatus {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uploading => "uploading",
            Self::Finalising => "finalising",
            Self::Live => "live",
            Self::Failed => "failed",
            Self::Superseded => "superseded",
            Self::Pruned => "pruned",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "uploading" => Some(Self::Uploading),
            "finalising" => Some(Self::Finalising),
            "live" => Some(Self::Live),
            "failed" => Some(Self::Failed),
            "superseded" => Some(Self::Superseded),
            "pruned" => Some(Self::Pruned),
            _ => None,
        }
    }
}

/// New durable metadata for one named Web Yard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewWebYard {
    /// Stable Yard identifier.
    pub id: String,
    /// Parent workspace identifier.
    pub workspace_id: String,
    /// Parent project identifier.
    pub project_id: String,
    /// Project-unique Yard name.
    pub name: Slug,
    /// Stable public host label.
    pub host_label: String,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
}

/// New durable metadata for one idempotent immutable deploy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewYardDeploy {
    /// Stable deploy identifier.
    pub id: String,
    /// Parent Yard identifier.
    pub yard_id: String,
    /// Parent workspace identifier.
    pub workspace_id: String,
    /// Parent project identifier.
    pub project_id: String,
    /// Client-generated idempotency identifier.
    pub client_deploy_id: String,
    /// Reserved manifest root.
    pub manifest_root: String,
    /// Immutable deployment host label.
    pub deployment_host_label: String,
    /// Whether unmatched extensionless paths use the root entry file.
    pub spa: bool,
    /// Whether extensionless paths resolve matching HTML files.
    pub clean_urls: bool,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
}

/// One immutable object selected for a finalised deploy manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewYardFile {
    /// Normalized relative URL path within the deploy.
    pub normalized_path: String,
    /// Immutable object version supplying the bytes.
    pub version_id: String,
    /// Committed object byte size.
    pub byte_size: u64,
}

/// Durable metadata for one named Web Yard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebYardRecord {
    /// Stable Yard identifier.
    pub id: String,
    /// Parent workspace identifier.
    pub workspace_id: String,
    /// Parent project identifier.
    pub project_id: String,
    /// Project-unique Yard name.
    pub name: Slug,
    /// Stable public host label.
    pub host_label: String,
    /// Deploy currently selected by the stable alias.
    pub current_deploy_id: Option<String>,
    /// Persisted lifecycle state.
    pub status: WebYardStatus,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Last stable-pointer change as Unix milliseconds.
    pub updated_at_ms: u64,
    /// Deletion time, when deleted.
    pub deleted_at_ms: Option<u64>,
}

/// Durable metadata for one immutable Web Yard deploy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardDeployRecord {
    /// Stable deploy identifier.
    pub id: String,
    /// Parent Yard identifier.
    pub yard_id: String,
    /// Parent workspace identifier.
    pub workspace_id: String,
    /// Parent project identifier.
    pub project_id: String,
    /// Client-generated idempotency identifier.
    pub client_deploy_id: String,
    /// Reserved logical manifest root.
    pub manifest_root: String,
    /// Immutable deployment host label.
    pub deployment_host_label: String,
    /// Whether unmatched extensionless paths use the root entry file.
    pub spa: bool,
    /// Whether extensionless paths resolve matching HTML files.
    pub clean_urls: bool,
    /// Persisted lifecycle state.
    pub status: YardDeployStatus,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Successful finalisation time, when available.
    pub finalised_at_ms: Option<u64>,
    /// Immutable manifest file count.
    pub file_count: u64,
    /// Immutable manifest byte total.
    pub total_bytes: u64,
}

/// One idempotent deploy start result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardStartRecord {
    /// Existing or newly created Yard.
    pub yard: WebYardRecord,
    /// Existing or newly reserved deploy.
    pub deploy: YardDeployRecord,
}

/// One successful finalise or rollback result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardDeploymentRecord {
    /// Selected Yard.
    pub yard: WebYardRecord,
    /// Finalised deploy selected by the operation.
    pub deploy: YardDeployRecord,
}

/// One authorized immutable file selected by a Yard host mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardFileTarget {
    /// Stored object supplying the response bytes.
    pub object: StoredObjectRecord,
    /// Whether the selected file is the custom not-found document.
    pub not_found_document: bool,
}

#[cfg(test)]
#[path = "yards_tests.rs"]
mod tests;
