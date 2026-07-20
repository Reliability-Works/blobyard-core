use crate::{NewAuditEvent, RepositoryError, StoredObjectRecord};

/// Persisted lifecycle state for one isolated preview capability.
pub type PreviewStatus = crate::repository::RevocableStatus;

/// Maximum portable relative path length for one immutable preview file.
pub const MAXIMUM_PREVIEW_PATH_BYTES: usize = 1_024;

/// Returns whether a preview path is a portable, normalized relative URL path.
#[must_use]
pub fn is_valid_preview_path(value: &str) -> bool {
    crate::is_valid_relative_path(value, MAXIMUM_PREVIEW_PATH_BYTES)
}

/// One immutable object selected for a preview manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewPreviewFile {
    /// Normalized relative URL path within the preview.
    pub normalized_path: String,
    /// Immutable object version supplying the bytes.
    pub version_id: String,
}

/// Validated input for one expiring isolated preview.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewPreview {
    /// Stable preview identifier.
    pub id: String,
    /// Workspace that owns the preview.
    pub workspace_id: String,
    /// Project containing the immutable manifest.
    pub project_id: String,
    /// Lowercase SHA-256 digest of the raw DNS host capability.
    pub capability_hash: String,
    /// Absolute capability expiry as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Durable creation timestamp as Unix milliseconds.
    pub created_at_ms: u64,
    /// Complete nonempty immutable manifest.
    pub files: Vec<NewPreviewFile>,
}

/// Redacted durable preview metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewRecord {
    /// Stable preview identifier.
    pub id: String,
    /// Workspace that owns the preview.
    pub workspace_id: String,
    /// Project containing the immutable manifest.
    pub project_id: String,
    /// Absolute capability expiry as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Persisted lifecycle state.
    pub status: PreviewStatus,
    /// Durable creation timestamp as Unix milliseconds.
    pub created_at_ms: u64,
    /// Revocation time, when revoked.
    pub revoked_at_ms: Option<u64>,
}

/// One authorized preview file and its immutable object metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewTarget {
    /// Redacted preview metadata.
    pub preview: PreviewRecord,
    /// Normalized relative URL path.
    pub normalized_path: String,
    /// Immutable stored object exposed at that path.
    pub object: StoredObjectRecord,
}

/// Durable standalone operations for isolated preview capabilities.
pub trait PreviewRepository: Send + Sync {
    /// Atomically snapshots one complete manifest and records its creation audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failures.
    fn create_preview(
        &self,
        preview: &NewPreview,
        event: &NewAuditEvent,
    ) -> Result<PreviewRecord, RepositoryError>;

    /// Lists redacted previews for one project in newest-first order.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures.
    fn list_previews(&self, project_id: &str) -> Result<Vec<PreviewRecord>, RepositoryError>;

    /// Reads one preview by stable identifier without exposing its capability digest.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or provider failures.
    fn preview_by_id(&self, preview_id: &str) -> Result<PreviewRecord, RepositoryError>;

    /// Resolves one active, unexpired preview capability and exact manifest path.
    ///
    /// # Errors
    ///
    /// Returns not-found for unknown, expired, revoked, or unavailable capabilities and files.
    fn preview_file_by_capability(
        &self,
        capability_hash: &str,
        normalized_path: &str,
        now_ms: u64,
    ) -> Result<PreviewTarget, RepositoryError>;

    /// Atomically revokes one project-owned preview and records its audit event.
    ///
    /// Returns `true` when this call revoked the preview and `false` when it was already revoked.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or provider failures.
    fn revoke_preview(
        &self,
        preview_id: &str,
        workspace_id: &str,
        project_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError>;
}

#[cfg(test)]
#[path = "previews_tests.rs"]
mod tests;
