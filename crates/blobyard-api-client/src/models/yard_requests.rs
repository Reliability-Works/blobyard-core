use super::encoding;
use blobyard_core::Slug;
use serde::{Deserialize, Serialize};

/// Starts an idempotent immutable deploy for a named Web Yard.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartYardDeployRequest {
    /// Workspace slug.
    pub workspace: Slug,
    /// Project slug.
    pub project: Slug,
    /// Project-unique Yard name.
    pub name: Slug,
    /// Client-generated stable deploy identifier.
    pub client_deploy_id: String,
    /// Whether unmatched extensionless paths use the root entry file.
    pub spa: bool,
    /// Whether extensionless paths resolve matching HTML files.
    pub clean_urls: bool,
    /// Explicit acknowledgement that the deployed files become public.
    pub public: bool,
}

impl StartYardDeployRequest {
    /// Encodes the deploy-start request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({
            "workspace": self.workspace,
            "project": self.project,
            "name": self.name,
            "clientDeployId": self.client_deploy_id,
            "spa": self.spa,
            "cleanUrls": self.clean_urls,
            "public": self.public,
        })
    }
}

/// Selects an already started Web Yard deploy.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct YardDeployMutationRequest {
    /// Stable server deploy identifier.
    pub deploy_id: String,
}

impl YardDeployMutationRequest {
    /// Encodes the deploy mutation.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "deployId": self.deploy_id })
    }
}

/// Marks an incomplete Web Yard deploy as failed.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FailYardDeployRequest {
    /// Stable server deploy identifier.
    pub deploy_id: String,
    /// Stable redaction-safe failure code.
    pub failure_code: String,
    /// Redaction-safe failure message.
    pub failure_message: String,
}

impl FailYardDeployRequest {
    /// Encodes the deploy failure.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({
            "deployId": self.deploy_id,
            "failureCode": self.failure_code,
            "failureMessage": self.failure_message,
        })
    }
}

/// Lists Web Yards in one project.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListWebYardsQuery {
    /// Workspace slug.
    pub workspace: Slug,
    /// Project slug.
    pub project: Slug,
}

impl ListWebYardsQuery {
    /// Encodes the scoped Yard-list query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::scoped_query(&self.workspace, &self.project, Vec::new())
    }
}

/// Lists immutable deploy history for one Web Yard.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListYardDeploysQuery {
    /// Stable Yard identifier.
    pub yard_id: String,
}

impl ListYardDeploysQuery {
    /// Encodes the deploy-history query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[("yardId", Some(self.yard_id))])
    }
}

/// Repoints a Web Yard alias to an earlier immutable deploy.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RollbackWebYardRequest {
    /// Stable Yard identifier.
    pub yard_id: String,
    /// Specific deploy identifier, or the previous eligible deploy when omitted.
    pub deploy_id: Option<String>,
}

impl RollbackWebYardRequest {
    /// Encodes the rollback request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        let mut body = serde_json::json!({ "yardId": self.yard_id });
        if let Some(deploy_id) = self.deploy_id {
            body["deployId"] = serde_json::Value::String(deploy_id);
        }
        body
    }
}

/// Deletes a Web Yard after client-side destructive confirmation.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteWebYardRequest {
    /// Stable Yard identifier.
    pub yard_id: String,
}

impl DeleteWebYardRequest {
    /// Encodes the deletion request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "yardId": self.yard_id })
    }
}
