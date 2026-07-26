#![allow(
    clippy::redundant_pub_crate,
    reason = "the private sibling tool-call parser dispatches to these Yard helpers"
)]

use crate::Scope;
use crate::tool_call::{optional_bool, required_string};
use serde_json::{Map, Value};

#[path = "yard_identity_call.rs"]
mod identity;

/// A validated MCP operation for public Web Yards.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebYardToolCall {
    /// Deploy a local static directory to a named Web Yard.
    DeployWebYard {
        /// CLI scope overrides.
        scope: Scope,
        /// Local static directory containing `index.html`.
        directory: String,
        /// Project-unique Web Yard name.
        yard: String,
        /// Whether SPA fallback is enabled.
        spa: bool,
        /// Whether clean HTML URLs are enabled.
        clean_urls: bool,
    },
    /// List Web Yards in the selected project.
    ListWebYards {
        /// CLI scope overrides.
        scope: Scope,
    },
    /// List immutable deploys for one Web Yard.
    ListYardDeploys {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
    },
    /// List active environments for one Web Yard.
    ListYardEnvironments {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
    },
    /// Show one Web Yard's effective visibility and active grants.
    GetYardAccess {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
    },
    /// Set one Web Yard's visibility policy.
    SetYardVisibility {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
        /// Requested audience.
        visibility: String,
    },
    /// Grant one principal scoped access to a Web Yard.
    GrantYardAccess {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
        /// Principal kind.
        principal_kind: String,
        /// Stable principal identifier.
        principal_id: String,
        /// Application roles granted to the principal.
        roles: Vec<String>,
        /// Optional environment restriction.
        environment_id: Option<String>,
        /// Optional RFC 3339 expiry.
        expires_at: Option<String>,
    },
    /// Revoke one Web Yard access grant.
    RevokeYardAccess {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
        /// Stable grant identifier.
        grant_id: String,
    },
    /// List Yard management-role assignments.
    ListYardManagementRoles {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
        /// Optional continuation cursor.
        cursor: Option<String>,
    },
    /// Create or change one Yard management-role assignment.
    SetYardManagementRole {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
        /// Stable active local-user identifier.
        user_id: String,
        /// Replacement management role.
        role: String,
    },
    /// Revoke one Yard management-role assignment.
    RevokeYardManagementRole {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
        /// Stable active local-user identifier.
        user_id: String,
    },
    /// Read one Yard's approved application policy.
    GetYardApplicationPolicy {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
    },
    /// Approve one Yard application policy.
    SetYardApplicationPolicy {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
        /// Canonical source-manifest digest.
        source_manifest_digest: String,
        /// Optional declared default role.
        default_role: Option<String>,
        /// Role-definition map.
        roles: Value,
    },
    /// Replace one Yard access grant's application roles.
    SetYardAccessRoles {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
        /// Stable active grant identifier.
        grant_id: String,
        /// Replacement application roles.
        roles: Vec<String>,
    },
    /// List retained browser sessions for one Web Yard.
    ListYardSessions {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
    },
    /// Revoke one retained Web Yard browser session.
    RevokeYardSession {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
        /// Stable Yard browser-session identifier.
        session_id: String,
    },
    /// Repoint a Web Yard to an earlier immutable deploy.
    RollbackWebYard {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
        /// Specific deploy identifier, or the previous deploy when omitted.
        deploy_id: Option<String>,
    },
    /// Delete a Web Yard after explicit destructive confirmation.
    DeleteWebYard {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
    },
}

pub(crate) fn is_yard_tool(name: &str) -> bool {
    matches!(
        name,
        "deploy_web_yard"
            | "list_web_yards"
            | "list_yard_deploys"
            | "list_yard_environments"
            | "get_yard_access"
            | "set_yard_visibility"
            | "grant_yard_access"
            | "revoke_yard_access"
            | "list_yard_management_roles"
            | "set_yard_management_role"
            | "revoke_yard_management_role"
            | "get_yard_application_policy"
            | "set_yard_application_policy"
            | "set_yard_access_roles"
            | "list_yard_sessions"
            | "revoke_yard_session"
            | "rollback_web_yard"
            | "delete_web_yard"
    )
}

pub(crate) fn parse_yard_call(
    name: &str,
    arguments: &Map<String, Value>,
    scope: Scope,
) -> Result<WebYardToolCall, String> {
    reject_unknown(name, arguments)?;
    if identity::is_tool(name) {
        return identity::parse(name, arguments, scope);
    }
    match name {
        "deploy_web_yard" => parse_deploy(scope, arguments),
        "list_web_yards" => Ok(WebYardToolCall::ListWebYards { scope }),
        "list_yard_deploys" => Ok(WebYardToolCall::ListYardDeploys {
            scope,
            yard: required_string(arguments, "yard")?,
        }),
        "list_yard_environments" => Ok(WebYardToolCall::ListYardEnvironments {
            scope,
            yard: required_string(arguments, "yard")?,
        }),
        "get_yard_access" => Ok(WebYardToolCall::GetYardAccess {
            scope,
            yard: required_string(arguments, "yard")?,
        }),
        "set_yard_visibility" => Ok(WebYardToolCall::SetYardVisibility {
            scope,
            yard: required_string(arguments, "yard")?,
            visibility: required_string(arguments, "visibility")?,
        }),
        "grant_yard_access" => Ok(WebYardToolCall::GrantYardAccess {
            scope,
            yard: required_string(arguments, "yard")?,
            principal_kind: required_string(arguments, "principal_kind")?,
            principal_id: required_string(arguments, "principal_id")?,
            roles: string_list(arguments, "roles")?,
            environment_id: crate::optional_string(arguments, "environment_id")?,
            expires_at: crate::optional_string(arguments, "expires_at")?,
        }),
        "revoke_yard_access" => Ok(WebYardToolCall::RevokeYardAccess {
            scope,
            yard: required_string(arguments, "yard")?,
            grant_id: required_string(arguments, "grant_id")?,
        }),
        "list_yard_sessions" => Ok(WebYardToolCall::ListYardSessions {
            scope,
            yard: required_string(arguments, "yard")?,
        }),
        "revoke_yard_session" => Ok(WebYardToolCall::RevokeYardSession {
            scope,
            yard: required_string(arguments, "yard")?,
            session_id: required_string(arguments, "session_id")?,
        }),
        "rollback_web_yard" => Ok(WebYardToolCall::RollbackWebYard {
            scope,
            yard: required_string(arguments, "yard")?,
            deploy_id: crate::optional_string(arguments, "deploy_id")?,
        }),
        "delete_web_yard" => parse_delete(scope, arguments),
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn parse_deploy(scope: Scope, arguments: &Map<String, Value>) -> Result<WebYardToolCall, String> {
    require_true(arguments, "public")?;
    Ok(WebYardToolCall::DeployWebYard {
        scope,
        directory: required_string(arguments, "directory")?,
        yard: required_string(arguments, "yard")?,
        spa: optional_bool(arguments, "spa")?.unwrap_or(false),
        clean_urls: optional_bool(arguments, "clean_urls")?.unwrap_or(false),
    })
}

fn parse_delete(scope: Scope, arguments: &Map<String, Value>) -> Result<WebYardToolCall, String> {
    require_true(arguments, "confirm")?;
    Ok(WebYardToolCall::DeleteWebYard {
        scope,
        yard: required_string(arguments, "yard")?,
    })
}

pub(super) fn string_list(
    arguments: &Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| format!("{key} must be an array of strings"))?;
    items
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|text| !text.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| format!("{key} must contain non-empty strings"))
        })
        .collect()
}

fn require_true(arguments: &Map<String, Value>, key: &str) -> Result<(), String> {
    match optional_bool(arguments, key)? {
        Some(true) => Ok(()),
        Some(false) => Err(format!("{key} must be true to confirm this operation")),
        None => Err(format!("missing required argument: {key}")),
    }
}

fn reject_unknown(name: &str, arguments: &Map<String, Value>) -> Result<(), String> {
    let specific: &[&str] = match name {
        "deploy_web_yard" => &["directory", "yard", "spa", "clean_urls", "public"],
        "list_yard_deploys"
        | "list_yard_environments"
        | "get_yard_access"
        | "get_yard_application_policy"
        | "list_yard_sessions" => &["yard"],
        "list_yard_management_roles" => &["yard", "cursor"],
        "set_yard_management_role" => &["yard", "user_id", "role"],
        "revoke_yard_management_role" => &["yard", "user_id"],
        "set_yard_application_policy" => {
            &["yard", "source_manifest_digest", "default_role", "roles"]
        }
        "set_yard_access_roles" => &["yard", "grant_id", "roles"],
        "set_yard_visibility" => &["yard", "visibility"],
        "grant_yard_access" => &[
            "yard",
            "principal_kind",
            "principal_id",
            "roles",
            "environment_id",
            "expires_at",
        ],
        "revoke_yard_access" => &["yard", "grant_id"],
        "revoke_yard_session" => &["yard", "session_id"],
        "delete_web_yard" => &["yard", "confirm"],
        "rollback_web_yard" => &["yard", "deploy_id"],
        _ => &[],
    };
    arguments
        .keys()
        .find(|key| {
            !matches!(key.as_str(), "workspace" | "project") && !specific.contains(&key.as_str())
        })
        .map_or(Ok(()), |key| Err(format!("unexpected argument: {key}")))
}
