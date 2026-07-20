use crate::Page;
use blobyard_core::Slug;
use serde::{Deserialize, Serialize};

/// Public lifecycle state for a Web Yard.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WebYardStatus {
    /// The Yard may serve its current deploy.
    Active,
    /// The Yard is unavailable because an administrator suspended it.
    Suspended,
}

/// Public lifecycle state for one immutable Web Yard deploy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum YardDeployStatus {
    /// The client is uploading the deploy manifest.
    Uploading,
    /// The server is validating and promoting the deploy.
    Finalising,
    /// The deploy is live at the Yard URL.
    Live,
    /// The deploy did not finish successfully.
    Failed,
    /// A newer deploy or rollback replaced this deploy.
    Superseded,
    /// Retention cleanup removed this deploy.
    Pruned,
}

/// Stable metadata for one named Web Yard.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebYardSummary {
    /// Deploy currently selected by the public alias.
    pub current_deploy_id: Option<String>,
    /// Allocated host label on the isolated user-content domain.
    pub host_label: String,
    /// Stable Yard identifier.
    pub id: String,
    /// Project-unique Yard name.
    pub name: Slug,
    /// Parent project identifier.
    pub project_id: String,
    /// Current lifecycle state.
    pub status: WebYardStatus,
    /// Stable public URL on the isolated user-content domain.
    pub url: String,
    /// Parent workspace identifier.
    pub workspace_id: String,
}

/// Stable metadata for one immutable Web Yard deploy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YardDeploySummary {
    /// Whether extensionless paths resolve matching HTML files.
    pub clean_urls: bool,
    /// Client-generated idempotency identifier.
    pub client_deploy_id: String,
    /// Creation timestamp in Unix milliseconds.
    pub created_at: u64,
    /// Immutable public URL bound to this exact deploy while it is retained.
    pub deployment_url: String,
    /// Number of files in the immutable manifest.
    pub file_count: u64,
    /// Successful finalisation timestamp in Unix milliseconds, when available.
    pub finalised_at: Option<u64>,
    /// Stable deploy identifier.
    pub id: String,
    /// Whether the Yard alias currently selects this deploy.
    pub is_current: bool,
    /// Whether unmatched extensionless paths use the root entry file.
    pub spa: bool,
    /// Current deploy lifecycle state.
    pub status: YardDeployStatus,
    /// Total manifest size in bytes.
    pub total_bytes: u64,
}

/// Upload reservation created for a Web Yard deploy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartYardDeployResponse {
    /// Stable deploy identifier.
    pub deploy_id: String,
    /// Immutable public URL reserved for this exact deploy.
    pub deployment_url: String,
    /// Allocated host label on the isolated user-content domain.
    pub host_label: String,
    /// Reserved logical root under which the client uploads the manifest.
    pub manifest_root: String,
    /// Current deploy lifecycle state.
    pub status: YardDeployStatus,
    /// Stable public Web Yard URL.
    pub url: String,
    /// Stable Yard identifier.
    pub yard_id: String,
    /// Project-unique Yard name.
    pub yard_name: Slug,
}

/// A successful finalise or rollback response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YardDeploymentResponse {
    /// Stable deploy identifier selected by the operation.
    pub deploy_id: String,
    /// Immutable public URL bound to the selected deploy while it is retained.
    pub deployment_url: String,
    /// Current deploy lifecycle state.
    pub status: YardDeployStatus,
    /// Stable public Web Yard alias selected by the operation.
    pub url: String,
}

/// Web Yard list response.
pub type WebYardPage = Page<WebYardSummary>;

/// Immutable Web Yard deploy-history response.
pub type YardDeployPage = Page<YardDeploySummary>;
