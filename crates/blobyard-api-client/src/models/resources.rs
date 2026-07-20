use super::encoding;
use crate::{Page, ProjectSummary, WorkspaceSummary};
use blobyard_core::{BlobyardUri, Slug};
use serde::{Deserialize, Serialize};

/// Cursor parameters shared by list operations.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CursorQuery {
    /// Opaque continuation cursor.
    pub cursor: Option<String>,
}

impl CursorQuery {
    /// Encodes an optional opaque cursor.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[("cursor", self.cursor)])
    }
}

/// Creates a workspace.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateWorkspaceRequest {
    /// Human-readable workspace name.
    pub name: String,
}

impl CreateWorkspaceRequest {
    /// Encodes the workspace creation body.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "name": self.name })
    }
}

/// Workspace list response.
pub type WorkspacePage = Page<WorkspaceSummary>;

/// Selects a workspace for project listing.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListProjectsQuery {
    /// Workspace slug.
    pub workspace: Slug,
    /// Opaque continuation cursor.
    pub cursor: Option<String>,
}

impl ListProjectsQuery {
    /// Encodes the bounded project-list query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[
            ("workspace", Some(self.workspace.to_string())),
            ("cursor", self.cursor),
        ])
    }
}

/// Creates a project.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateProjectRequest {
    /// Workspace slug.
    pub workspace: Slug,
    /// Human-readable project name.
    pub name: String,
}

impl CreateProjectRequest {
    /// Encodes the validated project request for the transport layer.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "workspace": self.workspace, "name": self.name })
    }
}

/// Project list response.
pub type ProjectPage = Page<ProjectSummary>;

/// An immutable object version visible to the caller.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectSummary {
    /// Canonical object URI.
    pub uri: BlobyardUri,
    /// Original safe filename.
    pub filename: String,
    /// Object byte length.
    pub size_bytes: u64,
    /// Server timestamp.
    pub created_at: String,
    /// Current object-version availability.
    pub availability: ObjectAvailability,
    /// Object ingestion source.
    pub source: ObjectSource,
}

/// Availability of an immutable object version.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectAvailability {
    /// Bytes are available for download.
    Available,
    /// Metadata remains after deletion.
    Deleted,
    /// Upload completion is pending.
    Pending,
    /// Metadata exists but bytes are unavailable.
    Unavailable,
}

/// Ingestion surface that created an object version.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectSource {
    /// GitHub Actions or another CI trust.
    Ci,
    /// Native CLI, SDK, or local API token.
    Cli,
    /// Public upload inbox.
    Inbox,
    /// Preview publication.
    Preview,
    /// Authenticated web dashboard.
    Web,
}

/// Lists object versions under an optional prefix.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListObjectsQuery {
    /// Workspace slug.
    pub workspace: Slug,
    /// Project slug.
    pub project: Slug,
    /// Optional logical-path prefix.
    pub prefix: Option<String>,
    /// Whether to include immutable historical versions.
    pub versions: bool,
    /// Opaque continuation cursor.
    pub cursor: Option<String>,
}

impl ListObjectsQuery {
    /// Encodes the bounded object-list query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::scoped_query(
            &self.workspace,
            &self.project,
            vec![
                ("prefix", self.prefix),
                ("versions", Some(self.versions.to_string())),
                ("cursor", self.cursor),
            ],
        )
    }
}

/// Object list response.
pub type ObjectPage = Page<ObjectSummary>;

/// Soft-deletes an object or immutable version.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteObjectRequest {
    /// Canonical object URI.
    pub uri: BlobyardUri,
}

impl DeleteObjectRequest {
    /// Encodes the validated deletion request for the transport layer.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "uri": self.uri })
    }
}

/// Confirms a soft deletion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteObjectResponse {
    /// Canonical deleted URI.
    pub uri: BlobyardUri,
    /// Whether the object is now deleted.
    pub deleted: bool,
}
