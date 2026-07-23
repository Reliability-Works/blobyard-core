use super::encoding;
use blobyard_core::{SecretString, Slug};
use serde::{Deserialize, Serialize};

/// Public lifecycle state for one local user.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LocalUserStatus {
    /// The user may sign in with an active key.
    Active,
    /// The user is tombstoned and admits nothing.
    Deactivated,
}

/// Stable metadata for one local user.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalUserSummary {
    /// Creation timestamp as RFC 3339.
    pub created_at: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Optional email unique among active users in the workspace.
    pub email: Option<String>,
    /// Stable user identifier.
    pub id: String,
    /// Non-secret prefix of the active sign-in key when one exists.
    pub login_key_prefix: Option<String>,
    /// Current lifecycle state.
    pub status: LocalUserStatus,
    /// Owning workspace identifier.
    pub workspace_id: String,
}

/// Lists local users in one workspace.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListLocalUsersQuery {
    /// Workspace slug.
    pub workspace: Slug,
}

impl ListLocalUsersQuery {
    /// Encodes the user-list query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[("workspace", Some(self.workspace.to_string()))])
    }
}

/// A successful local-user listing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListLocalUsersResponse {
    /// Users in the workspace, newest first.
    pub users: Vec<LocalUserSummary>,
}

/// Creates one local user with a first sign-in key.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateLocalUserRequest {
    /// Workspace slug.
    pub workspace: Slug,
    /// Human-readable display name.
    pub display_name: String,
    /// Optional email unique among active users in the workspace.
    pub email: Option<String>,
}

impl CreateLocalUserRequest {
    /// Encodes the creation request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        let mut body = serde_json::json!({
            "displayName": self.display_name,
            "workspace": self.workspace,
        });
        if let Some(email) = self.email {
            body["email"] = serde_json::Value::String(email);
        }
        body
    }
}

/// A successful creation carrying the raw sign-in key exactly once.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLocalUserResponse {
    /// Raw sign-in key returned once.
    pub login_key: SecretString,
    /// Non-secret prefix shown in listings.
    pub login_key_prefix: String,
    /// The created user.
    pub user: LocalUserSummary,
}

/// Replaces every active sign-in key for one local user.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResetLocalUserLoginKeyRequest {
    /// Stable user identifier.
    pub user_id: String,
}

impl ResetLocalUserLoginKeyRequest {
    /// Encodes the reset request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "userId": self.user_id })
    }
}

/// A successful reset carrying the raw replacement key exactly once.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalUserLoginKeyResponse {
    /// Raw sign-in key returned once.
    pub login_key: SecretString,
    /// Non-secret prefix shown in listings.
    pub login_key_prefix: String,
}

/// Deactivates one local user.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeactivateLocalUserRequest {
    /// Stable user identifier.
    pub user_id: String,
}

impl DeactivateLocalUserRequest {
    /// Encodes the deactivation request.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        serde_json::json!({ "userId": self.user_id })
    }
}
