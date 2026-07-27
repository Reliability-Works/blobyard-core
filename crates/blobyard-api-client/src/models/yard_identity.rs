use super::{YardAccessGrantSummary, encoding};
use blobyard_core::ApplicationPolicyGraph;
use serde::{Deserialize, Serialize};

/// One Yard-scoped human management role.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum YardManagementRole {
    /// Full Yard authority.
    Owner,
    /// Yard access and operational authority.
    Admin,
    /// Yard deployment and operational authority.
    Developer,
    /// Read-only Yard authority.
    Auditor,
}

/// Stable metadata for one Yard management-role assignment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YardManagementRoleAssignment {
    /// Assigned active local-user identifier.
    pub user_id: String,
    /// Assigned management role.
    pub role: YardManagementRole,
    /// Creation timestamp as RFC 3339.
    pub created_at: String,
    /// Last-change timestamp as RFC 3339.
    pub updated_at: String,
}

/// Lists one deterministic page of Yard management roles.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListYardManagementRolesQuery {
    /// Stable Yard identifier.
    pub yard_id: String,
    /// Opaque next-page cursor.
    pub cursor: Option<String>,
}

impl ListYardManagementRolesQuery {
    /// Encodes the role-list query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[("yardId", Some(self.yard_id)), ("cursor", self.cursor)])
    }
}

/// One deterministic management-role page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListYardManagementRolesResponse {
    /// Ordered page items.
    pub items: Vec<YardManagementRoleAssignment>,
    /// Opaque cursor for the next page.
    pub next_cursor: Option<String>,
}

/// Creates or changes one Yard management-role assignment.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetYardManagementRoleRequest {
    /// Stable Yard identifier.
    pub yard_id: String,
    /// Stable active local-user identifier.
    pub user_id: String,
    /// Replacement management role.
    pub role: YardManagementRole,
}

impl SetYardManagementRoleRequest {
    /// Encodes the role mutation.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({
            "role": self.role,
            "userId": self.user_id,
            "yardId": self.yard_id,
        })
    }
}

/// Revokes one Yard management-role assignment.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeYardManagementRoleRequest {
    /// Stable Yard identifier.
    pub yard_id: String,
    /// Stable active local-user identifier.
    pub user_id: String,
}

impl RevokeYardManagementRoleRequest {
    /// Encodes the role revocation.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "userId": self.user_id, "yardId": self.yard_id })
    }
}

/// Reads the approved application policy for one Yard.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetYardApplicationPolicyQuery {
    /// Stable Yard identifier.
    pub yard_id: String,
}

impl GetYardApplicationPolicyQuery {
    /// Encodes the policy-read query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[("yardId", Some(self.yard_id))])
    }
}

/// One current owner-approved application policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YardApplicationPolicy {
    /// Monotonic policy revision.
    pub revision: u64,
    /// Digest of the canonical source manifest.
    pub source_manifest_digest: String,
    /// Canonical declared role graph.
    #[serde(flatten)]
    pub graph: ApplicationPolicyGraph,
    /// Approval timestamp as RFC 3339.
    pub approved_at: String,
    /// Safe approving operator principal identifier.
    pub approved_by_principal_id: String,
}

/// Approved policy read result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YardApplicationPolicyResponse {
    /// Current policy, or `null` before first approval.
    pub policy: Option<YardApplicationPolicy>,
}

/// Approves one canonical Yard application policy.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetYardApplicationPolicyRequest {
    /// Stable Yard identifier.
    pub yard_id: String,
    /// Digest of the canonical source manifest.
    pub source_manifest_digest: String,
    /// Declared application policy.
    #[serde(flatten)]
    pub policy: ApplicationPolicyGraph,
}

impl SetYardApplicationPolicyRequest {
    /// Encodes the policy mutation.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({
            "defaultRole": self.policy.default_role,
            "roles": self.policy.roles,
            "sourceManifestDigest": self.source_manifest_digest,
            "yardId": self.yard_id,
        })
    }
}

/// Replaces one active Yard grant's application roles.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetYardAccessRolesRequest {
    /// Stable Yard identifier.
    pub yard_id: String,
    /// Stable active grant identifier.
    pub grant_id: String,
    /// Replacement declared role names.
    pub app_roles: Vec<String>,
}

impl SetYardAccessRolesRequest {
    /// Encodes the application-role mutation.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({
            "appRoles": self.app_roles,
            "grantId": self.grant_id,
            "yardId": self.yard_id,
        })
    }
}

/// Updated Yard access grant.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetYardAccessRolesResponse {
    /// Updated active grant.
    pub grant: YardAccessGrantSummary,
}

#[cfg(test)]
mod tests {
    use super::{
        GetYardApplicationPolicyQuery, ListYardManagementRolesQuery,
        RevokeYardManagementRoleRequest, SetYardAccessRolesRequest,
        SetYardApplicationPolicyRequest, SetYardManagementRoleRequest, YardManagementRole,
    };
    use blobyard_core::{ApplicationPolicyGraph, ApplicationRoleDefinition};
    use std::collections::BTreeMap;

    #[test]
    fn identity_queries_encode_only_supplied_fields() {
        assert_eq!(
            ListYardManagementRolesQuery {
                yard_id: "yard_docs".to_owned(),
                cursor: None,
            }
            .into_query(),
            "yardId=yard_docs"
        );
        assert_eq!(
            ListYardManagementRolesQuery {
                yard_id: "yard docs".to_owned(),
                cursor: Some("next/page".to_owned()),
            }
            .into_query(),
            "yardId=yard+docs&cursor=next%2Fpage"
        );
        assert_eq!(
            GetYardApplicationPolicyQuery {
                yard_id: "yard_docs".to_owned(),
            }
            .into_query(),
            "yardId=yard_docs"
        );
    }

    #[test]
    fn identity_mutations_encode_the_public_contract() {
        assert_eq!(
            SetYardManagementRoleRequest {
                yard_id: "yard_docs".to_owned(),
                user_id: "user_owner".to_owned(),
                role: YardManagementRole::Owner,
            }
            .into_json(),
            serde_json::json!({
                "role": "owner",
                "userId": "user_owner",
                "yardId": "yard_docs",
            })
        );
        assert_eq!(
            RevokeYardManagementRoleRequest {
                yard_id: "yard_docs".to_owned(),
                user_id: "user_owner".to_owned(),
            }
            .into_json(),
            serde_json::json!({
                "userId": "user_owner",
                "yardId": "yard_docs",
            })
        );
        assert_eq!(
            SetYardAccessRolesRequest {
                yard_id: "yard_docs".to_owned(),
                grant_id: "grant_reader".to_owned(),
                app_roles: vec!["viewer".to_owned()],
            }
            .into_json(),
            serde_json::json!({
                "appRoles": ["viewer"],
                "grantId": "grant_reader",
                "yardId": "yard_docs",
            })
        );
    }

    #[test]
    fn application_policy_mutation_flattens_the_graph() {
        let policy = ApplicationPolicyGraph {
            default_role: Some("viewer".to_owned()),
            roles: BTreeMap::from([(
                "viewer".to_owned(),
                ApplicationRoleDefinition {
                    inherits: Vec::new(),
                    permissions: vec!["content.read".to_owned()],
                },
            )]),
        };
        assert_eq!(
            SetYardApplicationPolicyRequest {
                yard_id: "yard_docs".to_owned(),
                source_manifest_digest: "a".repeat(64),
                policy,
            }
            .into_json(),
            serde_json::json!({
                "defaultRole": "viewer",
                "roles": {
                    "viewer": {
                        "inherits": [],
                        "permissions": ["content.read"],
                    },
                },
                "sourceManifestDigest": "a".repeat(64),
                "yardId": "yard_docs",
            })
        );
    }
}
