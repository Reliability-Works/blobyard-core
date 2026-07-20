use super::encoding;
use crate::Page;
use blobyard_core::{SecretString, Slug};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

/// Creates an isolated static preview.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatePreviewRequest {
    /// Workspace slug.
    pub workspace: Slug,
    /// Project slug.
    pub project: Slug,
    /// Immutable uploaded manifest identifier.
    pub manifest_id: String,
    /// Requested lifetime string.
    pub expires: Option<String>,
}

impl CreatePreviewRequest {
    /// Encodes the immutable preview-manifest request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({
            "workspace": self.workspace,
            "project": self.project,
            "manifestId": self.manifest_id,
            "expires": self.expires,
        })
    }
}

/// One-time preview capability response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePreviewResponse {
    /// Stable preview identifier.
    pub id: String,
    /// Isolated preview URL, returned once.
    pub preview_url: SecretString,
    /// Absolute expiry timestamp.
    pub expires_at: String,
}

/// Selects a project for preview listing.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListPreviewsQuery {
    /// Workspace slug.
    pub workspace: Slug,
    /// Project slug.
    pub project: Slug,
}

impl ListPreviewsQuery {
    /// Encodes the project scope.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::scoped_query(&self.workspace, &self.project, Vec::new())
    }
}

/// Redacted preview metadata returned to authenticated listings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSummary {
    /// Stable preview identifier.
    pub id: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Absolute expiry timestamp.
    pub expires_at: String,
    /// Current lifecycle state.
    pub status: String,
}

/// Preview list response.
pub type PreviewPage = Page<PreviewSummary>;

/// Revokes a preview by stable identifier.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokePreviewRequest {
    /// Stable preview identifier, never the capability token.
    pub preview_id: String,
}

impl RevokePreviewRequest {
    /// Encodes the stable preview revocation identifier.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "previewId": self.preview_id })
    }
}

/// Resolves a preview capability and normalized path.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvePreviewQuery {
    /// Raw preview capability.
    pub token: SecretString,
    /// Normalized relative asset path.
    pub path: String,
}

impl ResolvePreviewQuery {
    /// Encodes the preview capability and normalized asset path.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[
            ("token", Some(self.token.expose_secret().to_owned())),
            ("path", Some(self.path)),
        ])
    }
}

/// Authorized preview object metadata for the edge service.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewObjectResponse {
    /// Opaque physical-object handle for the edge service.
    pub object_handle: String,
    /// Server-selected content type.
    pub content_type: String,
    /// Maximum authorization cache duration.
    pub max_age_seconds: u8,
}

/// Creates a guest upload inbox.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateInboxRequest {
    /// Workspace slug.
    pub workspace: Slug,
    /// Project slug.
    pub project: Slug,
    /// Human-readable inbox name.
    pub name: String,
    /// Requested lifetime string.
    pub expires: Option<String>,
}

impl CreateInboxRequest {
    /// Encodes the validated inbox request for the transport layer.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({
            "workspace": self.workspace,
            "project": self.project,
            "name": self.name,
            "expires": self.expires,
        })
    }
}

/// One-time inbox capability response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInboxResponse {
    /// Stable inbox identifier.
    pub id: String,
    /// Raw public inbox URL, returned once.
    pub inbox_url: SecretString,
    /// Absolute expiry timestamp.
    pub expires_at: String,
}

/// Redacted inbox metadata returned to authenticated listings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxSummary {
    /// Stable inbox identifier.
    pub id: String,
    /// Human-readable inbox name.
    pub name: String,
    /// Absolute expiry timestamp.
    pub expires_at: String,
    /// Whether the inbox is revoked.
    pub revoked: bool,
}

/// Inbox list response.
pub type InboxPage = Page<InboxSummary>;

/// Selects a project for inbox listing.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListInboxesQuery {
    /// Workspace slug.
    pub workspace: Slug,
    /// Project slug.
    pub project: Slug,
    /// Opaque continuation cursor.
    pub cursor: Option<String>,
}

impl ListInboxesQuery {
    /// Encodes the bounded inbox-list query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::scoped_query(
            &self.workspace,
            &self.project,
            vec![("cursor", self.cursor)],
        )
    }
}

/// Revokes an inbox by stable identifier.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeInboxRequest {
    /// Stable inbox identifier, never the capability token.
    pub inbox_id: String,
}

impl RevokeInboxRequest {
    /// Encodes the validated inbox revocation for the transport layer.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "inboxId": self.inbox_id })
    }
}

/// Resolves a public inbox capability.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveInboxQuery {
    /// Raw inbox capability.
    pub token: SecretString,
}

impl ResolveInboxQuery {
    /// Encodes the raw inbox capability.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[("token", Some(self.token.expose_secret().to_owned()))])
    }
}

/// Safe public inbox metadata and limits.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxMetadata {
    /// Human-readable inbox name.
    pub name: String,
    /// Maximum number of files accepted.
    pub max_files: u32,
    /// Maximum total accepted bytes.
    pub max_bytes: u64,
    /// Absolute expiry timestamp.
    pub expires_at: String,
    /// Whether another upload may be reserved.
    pub upload_available: bool,
}

/// Selects a project retention policy.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetentionQuery {
    /// Workspace slug.
    pub workspace: Slug,
    /// Project slug.
    pub project: Slug,
}

impl RetentionQuery {
    /// Encodes the bounded retention query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::scoped_query(&self.workspace, &self.project, Vec::new())
    }
}

/// A deterministic project retention policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetentionPolicy {
    /// Number of newest matching versions preserved.
    pub keep_latest: NonZeroU32,
    /// Optional explicit branch-provenance glob.
    pub branch_glob: Option<String>,
    /// Optional normalized logical-path glob.
    pub path_glob: Option<String>,
}

/// Replaces a project retention policy.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetRetentionRequest {
    /// Workspace slug.
    pub workspace: Slug,
    /// Project slug.
    pub project: Slug,
    /// Replacement policy.
    pub policy: RetentionPolicy,
}

impl SetRetentionRequest {
    /// Encodes the validated retention policy for the transport layer.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        let mut request = serde_json::json!({
            "workspace": self.workspace,
            "project": self.project,
            "keepLatest": self.policy.keep_latest,
        });
        for (name, value) in [
            ("branch", self.policy.branch_glob),
            ("path", self.policy.path_glob),
        ] {
            if let Some(value) = value {
                request[name] = serde_json::Value::String(value);
            }
        }
        request
    }
}
