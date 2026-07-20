use crate::{NewAuditEvent, NewDownloadGrant, RepositoryError, StoredObjectRecord};

/// Persisted lifecycle state for one public share.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShareStatus {
    /// The capability may still resolve and issue downloads.
    Active,
    /// The configured download limit has been consumed.
    Exhausted,
    /// An authenticated operator revoked the capability.
    Revoked,
}

impl ShareStatus {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Exhausted => "exhausted",
            Self::Revoked => "revoked",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "exhausted" => Some(Self::Exhausted),
            "revoked" => Some(Self::Revoked),
            _ => None,
        }
    }
}

/// Validated input for one expiring public share capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewShare {
    /// Stable share identifier.
    pub id: String,
    /// Workspace that owns the share.
    pub workspace_id: String,
    /// Immutable object version exposed by the capability.
    pub version_id: String,
    /// Lowercase SHA-256 digest of the raw capability.
    pub capability_hash: String,
    /// Absolute capability expiry as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Optional maximum number of completed download grants.
    pub maximum_downloads: Option<u64>,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
}

/// Redacted durable share metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShareRecord {
    /// Stable share identifier.
    pub id: String,
    /// Workspace that owns the share.
    pub workspace_id: String,
    /// Immutable object version, when it still exists.
    pub version_id: Option<String>,
    /// Absolute capability expiry as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Persisted lifecycle state.
    pub status: ShareStatus,
    /// Number of completed download grants.
    pub consumed_count: u64,
    /// Optional maximum number of completed download grants.
    pub maximum_downloads: Option<u64>,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Revocation time, when revoked.
    pub revoked_at_ms: Option<u64>,
}

/// One active public share and its immutable object metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShareTarget {
    /// Redacted share metadata.
    pub share: ShareRecord,
    /// Stored object version exposed by the share.
    pub object: StoredObjectRecord,
}

/// Durable standalone operations for public share capabilities.
pub trait SharingRepository: Send + Sync {
    /// Atomically creates one share and its audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failures.
    fn create_share(
        &self,
        share: &NewShare,
        event: &NewAuditEvent,
    ) -> Result<ShareRecord, RepositoryError>;

    /// Lists redacted shares for one workspace in newest-first order.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures.
    fn list_shares(&self, workspace_id: &str) -> Result<Vec<ShareRecord>, RepositoryError>;

    /// Resolves one active or exhausted, unexpired capability and its object.
    ///
    /// # Errors
    ///
    /// Returns not-found for unknown, expired, revoked, or unavailable capabilities.
    fn share_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<ShareTarget, RepositoryError>;

    /// Atomically issues one share download, increments consumption, and records audit.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failures.
    fn issue_share_download(
        &self,
        capability_hash: &str,
        now_ms: u64,
        grant: &NewDownloadGrant,
        event: &NewAuditEvent,
    ) -> Result<ShareTarget, RepositoryError>;

    /// Atomically revokes one workspace-owned share and records audit.
    ///
    /// Returns `true` when this call revoked the share and `false` when it was already revoked.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or provider failures.
    fn revoke_share(
        &self,
        share_id: &str,
        workspace_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError>;
}

#[cfg(test)]
#[path = "sharing_tests.rs"]
mod tests;
