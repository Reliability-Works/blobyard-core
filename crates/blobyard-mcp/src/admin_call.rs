use serde_json::{Map, Value};

use crate::{Scope, optional_string};

#[derive(Clone, Debug, Eq, PartialEq)]
/// A validated administration operation requested through MCP.
pub enum AdminToolCall {
    /// List workspace audit events.
    ListAudit {
        /// Workspace override.
        scope: Scope,
        /// Optional pagination cursor.
        cursor: Option<String>,
    },
    /// List workspace members.
    ListMembers {
        /// Workspace override.
        scope: Scope,
    },
    /// List workspace invitations.
    ListInvites {
        /// Workspace override.
        scope: Scope,
    },
    /// Invite a workspace member.
    CreateInvite {
        /// Workspace override.
        scope: Scope,
        /// Recipient email address.
        email: String,
        /// Workspace role.
        role: String,
    },
    /// Revoke a workspace invitation.
    RevokeInvite {
        /// Workspace override.
        scope: Scope,
        /// Invitation identifier.
        invite_id: String,
        /// Explicit destructive confirmation.
        confirmed: bool,
    },
    /// Change a workspace member role.
    UpdateMemberRole {
        /// Workspace override.
        scope: Scope,
        /// Target user identifier.
        user_id: String,
        /// New workspace role.
        role: String,
        /// Explicit destructive confirmation.
        confirmed: bool,
    },
    /// Remove a workspace member.
    RemoveMember {
        /// Workspace override.
        scope: Scope,
        /// Target user identifier.
        user_id: String,
        /// Explicit destructive confirmation.
        confirmed: bool,
    },
    /// List redacted API-token metadata.
    ListApiTokens {
        /// Optional scope override retained for a uniform tool contract.
        scope: Scope,
    },
    /// Revoke an API token.
    RevokeApiToken {
        /// Optional scope override retained for a uniform tool contract.
        scope: Scope,
        /// Token identifier.
        token_id: String,
        /// Explicit destructive confirmation.
        confirmed: bool,
    },
    /// List GitHub OIDC trusts.
    ListCiTrusts {
        /// Workspace override.
        scope: Scope,
    },
    /// Create a GitHub OIDC trust.
    CreateCiTrust {
        /// Workspace and optional project override.
        scope: Scope,
        /// Trusted GitHub repository.
        repository: String,
        /// Trusted workflow path.
        workflow_path: String,
        /// Trusted workflow revision.
        workflow_ref: String,
        /// Allowed Git ref pattern.
        allowed_ref_glob: String,
        /// Allowed Blob Yard operations.
        allowed_actions: Vec<String>,
        /// Optional GitHub environment.
        environment: Option<String>,
    },
    /// Revoke a GitHub OIDC trust.
    RevokeCiTrust {
        /// Optional scope override retained for a uniform tool contract.
        scope: Scope,
        /// Trust identifier.
        trust_id: String,
        /// Explicit destructive confirmation.
        confirmed: bool,
    },
    /// List active CLI sessions.
    ListCliSessions {
        /// Optional scope override retained for a uniform tool contract.
        scope: Scope,
    },
    /// Revoke a CLI session.
    RevokeCliSession {
        /// Optional scope override retained for a uniform tool contract.
        scope: Scope,
        /// Session identifier.
        session_id: String,
        /// Explicit destructive confirmation.
        confirmed: bool,
    },
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "sibling modules consume this parser while the module stays crate-internal"
)]
pub(crate) fn is_admin_tool(name: &str) -> bool {
    matches!(
        name,
        "list_audit"
            | "list_members"
            | "list_invites"
            | "create_invite"
            | "revoke_invite"
            | "update_member_role"
            | "remove_member"
            | "list_api_tokens"
            | "revoke_api_token"
            | "list_ci_trusts"
            | "create_ci_trust"
            | "revoke_ci_trust"
            | "list_cli_sessions"
            | "revoke_cli_session"
    )
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "sibling modules consume this parser while the module stays crate-internal"
)]
pub(crate) fn parse_admin_call(
    name: &str,
    arguments: &Map<String, Value>,
    scope: Scope,
) -> Result<AdminToolCall, String> {
    reject_unknown(name, arguments)?;
    match name {
        "list_audit" => Ok(AdminToolCall::ListAudit {
            scope,
            cursor: optional_string(arguments, "cursor")?,
        }),
        "list_members" => Ok(AdminToolCall::ListMembers { scope }),
        "list_invites" => Ok(AdminToolCall::ListInvites { scope }),
        "create_invite" => Ok(AdminToolCall::CreateInvite {
            scope,
            email: required_string(arguments, "email")?,
            role: selected(arguments, "role", &["admin", "member", "owner"])?,
        }),
        "list_api_tokens" => Ok(AdminToolCall::ListApiTokens { scope }),
        "list_ci_trusts" => Ok(AdminToolCall::ListCiTrusts { scope }),
        "create_ci_trust" => parse_ci_trust(arguments, scope),
        "list_cli_sessions" => Ok(AdminToolCall::ListCliSessions { scope }),
        "revoke_invite" | "update_member_role" | "remove_member" | "revoke_api_token"
        | "revoke_ci_trust" | "revoke_cli_session" => {
            parse_confirmed_admin_call(name, arguments, scope)
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn parse_confirmed_admin_call(
    name: &str,
    arguments: &Map<String, Value>,
    scope: Scope,
) -> Result<AdminToolCall, String> {
    require_confirmation(arguments)?;
    match name {
        "revoke_invite" => Ok(AdminToolCall::RevokeInvite {
            scope,
            invite_id: required_string(arguments, "invite_id")?,
            confirmed: true,
        }),
        "update_member_role" => Ok(AdminToolCall::UpdateMemberRole {
            scope,
            user_id: required_string(arguments, "user_id")?,
            confirmed: true,
            role: selected(arguments, "role", &["admin", "member", "owner"])?,
        }),
        "remove_member" => Ok(AdminToolCall::RemoveMember {
            scope,
            user_id: required_string(arguments, "user_id")?,
            confirmed: true,
        }),
        "revoke_api_token" => Ok(AdminToolCall::RevokeApiToken {
            scope,
            token_id: required_string(arguments, "token_id")?,
            confirmed: true,
        }),
        "revoke_ci_trust" => Ok(AdminToolCall::RevokeCiTrust {
            scope,
            trust_id: required_string(arguments, "trust_id")?,
            confirmed: true,
        }),
        "revoke_cli_session" => Ok(AdminToolCall::RevokeCliSession {
            scope,
            session_id: required_string(arguments, "session_id")?,
            confirmed: true,
        }),
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn parse_ci_trust(arguments: &Map<String, Value>, scope: Scope) -> Result<AdminToolCall, String> {
    Ok(AdminToolCall::CreateCiTrust {
        scope,
        repository: required_string(arguments, "repository")?,
        workflow_path: required_string(arguments, "workflow_path")?,
        workflow_ref: required_string(arguments, "workflow_ref")?,
        allowed_ref_glob: required_string(arguments, "allowed_ref_glob")?,
        allowed_actions: required_strings(arguments, "allowed_actions")?,
        environment: optional_string(arguments, "environment")?,
    })
}

fn reject_unknown(name: &str, arguments: &Map<String, Value>) -> Result<(), String> {
    crate::reject_unknown_arguments(arguments, specific_keys(name))
}

fn specific_keys(name: &str) -> &'static [&'static str] {
    match name {
        "list_audit" => &["cursor"],
        "create_invite" => &["email", "role"],
        "revoke_invite" => &["confirm", "invite_id"],
        "update_member_role" => &["confirm", "role", "user_id"],
        "remove_member" => &["confirm", "user_id"],
        "revoke_api_token" => &["confirm", "token_id"],
        "create_ci_trust" => &[
            "allowed_actions",
            "allowed_ref_glob",
            "environment",
            "repository",
            "workflow_path",
            "workflow_ref",
        ],
        "revoke_ci_trust" => &["confirm", "trust_id"],
        "revoke_cli_session" => &["confirm", "session_id"],
        _ => &[],
    }
}

fn require_confirmation(arguments: &Map<String, Value>) -> Result<(), String> {
    match arguments.get("confirm").and_then(Value::as_bool) {
        Some(true) => Ok(()),
        Some(false) => Err("confirm must be true to confirm this operation".to_owned()),
        None if arguments.contains_key("confirm") => Err("confirm must be a boolean".to_owned()),
        None => Err("missing required argument: confirm".to_owned()),
    }
}

fn required_string(arguments: &Map<String, Value>, key: &str) -> Result<String, String> {
    optional_string(arguments, key)?
        .map_or_else(|| Err(format!("missing required argument: {key}")), Ok)
}

fn selected(arguments: &Map<String, Value>, key: &str, allowed: &[&str]) -> Result<String, String> {
    let value = required_string(arguments, key)?;
    allowed
        .contains(&value.as_str())
        .then_some(value)
        .ok_or_else(|| format!("{key} is not valid"))
}

fn required_strings(arguments: &Map<String, Value>, key: &str) -> Result<Vec<String>, String> {
    let values = arguments
        .get(key)
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty())
        .ok_or_else(|| format!("{key} must be a non-empty string array"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|text| !text.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| format!("{key} must contain only non-empty strings"))
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
fn arguments(value: &Value) -> Map<String, Value> {
    value.as_object().cloned().expect("object arguments")
}

#[cfg(test)]
#[path = "admin_call_confirmation_tests.rs"]
mod confirmation_tests;
#[cfg(test)]
#[path = "admin_call_tests.rs"]
mod tests;
