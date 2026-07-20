use blobyard_core::Slug;
use serde::{Deserialize, Serialize};

/// An empty successful response.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyResponse {}

/// Confirms that a project retention policy is disabled.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearRetentionResponse {
    /// Whether the policy is now disabled.
    pub cleared: bool,
}

/// A cursor-paginated list.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    items: Vec<T>,
    #[serde(default)]
    next_cursor: Option<String>,
}

impl<T> Page<T> {
    /// Creates a page.
    #[must_use]
    pub const fn new(items: Vec<T>, next_cursor: Option<String>) -> Self {
        Self { items, next_cursor }
    }

    /// Returns the page items.
    #[must_use]
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// Returns the continuation cursor.
    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
}

/// Service health and build identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    status: String,
    version: String,
}

impl HealthResponse {
    /// Returns the service status.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Returns the deployed service version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// A workspace visible to the authenticated principal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummary {
    id: String,
    slug: Slug,
    name: String,
}

impl WorkspaceSummary {
    /// Returns the workspace identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the workspace slug.
    #[must_use]
    pub const fn slug(&self) -> &Slug {
        &self.slug
    }

    /// Returns the display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// A project visible to the authenticated principal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    id: String,
    workspace_slug: Slug,
    slug: Slug,
    name: String,
}

impl ProjectSummary {
    /// Returns the project identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the parent workspace slug.
    #[must_use]
    pub const fn workspace_slug(&self) -> &Slug {
        &self.workspace_slug
    }

    /// Returns the project slug.
    #[must_use]
    pub const fn slug(&self) -> &Slug {
        &self.slug
    }

    /// Returns the display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}
