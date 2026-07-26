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

/// Lists active environments for one Web Yard.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListYardEnvironmentsQuery {
    /// Stable Yard identifier.
    pub yard_id: String,
}

impl ListYardEnvironmentsQuery {
    /// Encodes the environment-list query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[("yardId", Some(self.yard_id))])
    }
}

/// Reads one Web Yard's effective visibility and active grants.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetYardAccessQuery {
    /// Stable Yard identifier.
    pub yard_id: String,
}

impl GetYardAccessQuery {
    /// Encodes the access-read query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[("yardId", Some(self.yard_id))])
    }
}

/// Sets one Web Yard's visibility policy.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetYardVisibilityRequest {
    /// Stable Yard identifier.
    pub yard_id: String,
    /// Requested audience.
    pub visibility: super::YardVisibility,
}

impl SetYardVisibilityRequest {
    /// Encodes the visibility request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "visibility": self.visibility, "yardId": self.yard_id })
    }
}

/// Grants one principal scoped access to a Web Yard.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantYardAccessRequest {
    /// Stable Yard identifier.
    pub yard_id: String,
    /// Kind of admitted principal.
    pub principal_kind: super::YardAccessPrincipalKind,
    /// Stable identifier of the admitted principal.
    pub principal_id: String,
    /// Application roles the manifest declares.
    pub app_roles: Vec<String>,
    /// Optional single-environment restriction.
    pub environment_id: Option<String>,
    /// Optional RFC 3339 expiry.
    pub expires_at: Option<String>,
}

impl GrantYardAccessRequest {
    /// Encodes the grant request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        let mut body = serde_json::json!({
            "appRoles": self.app_roles,
            "principalId": self.principal_id,
            "principalKind": self.principal_kind,
            "yardId": self.yard_id,
        });
        if let Some(environment_id) = self.environment_id {
            body["environmentId"] = serde_json::Value::String(environment_id);
        }
        if let Some(expires_at) = self.expires_at {
            body["expiresAt"] = serde_json::Value::String(expires_at);
        }
        body
    }
}

/// Revokes one Web Yard access grant.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeYardAccessRequest {
    /// Stable Yard identifier.
    pub yard_id: String,
    /// Stable grant identifier.
    pub grant_id: String,
}

impl RevokeYardAccessRequest {
    /// Encodes the revocation request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "grantId": self.grant_id, "yardId": self.yard_id })
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
