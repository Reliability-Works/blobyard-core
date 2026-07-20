use super::encoding;
use crate::Page;
use blobyard_core::{BlobyardUri, SecretString, Slug};
use serde::{Deserialize, Serialize};

/// Creates an expiring public share.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateShareRequest {
    /// Immutable object target.
    pub target: BlobyardUri,
    /// Requested lifetime string.
    pub expires: Option<String>,
    /// Optional notification recipient.
    pub notify: Option<String>,
}

impl CreateShareRequest {
    /// Encodes the validated share request for the transport layer.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({
            "target": self.target,
            "expires": self.expires,
            "notify": self.notify,
        })
    }
}

/// Notification capture or delivery outcome for a newly created share.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareNotificationStatus {
    /// The hosted service durably captured the notification request.
    Captured,
    /// The hosted service suppressed a duplicate notification request.
    Deduplicated,
    /// Notification delivery failed, or delivery is unavailable in standalone Core.
    Failed,
    /// The caller did not request a notification.
    NotRequested,
    /// The hosted service queued the notification for delivery.
    Queued,
}

/// One-time share capability response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateShareResponse {
    /// Stable share identifier.
    pub id: String,
    /// Raw share URL, returned once.
    pub share_url: SecretString,
    /// Absolute expiry timestamp.
    pub expires_at: String,
    /// Notification capture or delivery outcome.
    pub notification_status: ShareNotificationStatus,
}

/// Resolves a public share capability.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveShareQuery {
    /// Raw share capability.
    pub token: SecretString,
}

impl ResolveShareQuery {
    /// Encodes the raw capability without exposing it through debug output.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[("token", Some(self.token.expose_secret().to_owned()))])
    }
}

/// Safe public share metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareMetadata {
    /// Safe filename.
    pub filename: String,
    /// Object size in bytes.
    pub size_bytes: u64,
    /// Coarse server-selected content type class.
    pub content_type_class: String,
    /// Absolute expiry timestamp.
    pub expires_at: String,
    /// Whether another download may be issued.
    pub download_available: bool,
}

/// Requests a signed download through a share capability.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShareDownloadRequest {
    /// Raw share capability.
    pub token: SecretString,
}

impl ShareDownloadRequest {
    /// Encodes the share download capability body.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "token": self.token })
    }
}

/// Revokes a share by stable identifier.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeShareRequest {
    /// Stable share identifier, never the capability token.
    pub share_id: String,
}

impl RevokeShareRequest {
    /// Encodes the stable share revocation identifier.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "shareId": self.share_id })
    }
}

/// Selects a workspace for share listing.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListSharesQuery {
    /// Workspace slug.
    pub workspace: Slug,
}

impl ListSharesQuery {
    /// Encodes the workspace scope.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[("workspace", Some(self.workspace.as_str().to_owned()))])
    }
}

/// Redacted share metadata returned to authenticated listings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareSummary {
    /// Stable share identifier.
    pub id: String,
    /// Absolute expiry timestamp.
    pub expires_at: String,
    /// Current lifecycle state.
    pub status: String,
    /// Number of completed downloads.
    pub consumed_count: u64,
    /// Optional maximum download count.
    pub maximum_downloads: Option<u64>,
}

/// Share list response.
pub type SharePage = Page<ShareSummary>;
