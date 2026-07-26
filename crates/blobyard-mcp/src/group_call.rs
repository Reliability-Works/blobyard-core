use serde_json::{Map, Value};

use super::{require_confirmation, required_string};
use crate::{Scope, optional_string};

/// A validated workspace-group operation requested through MCP.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GroupToolCall {
    /// List groups in one workspace.
    List {
        /// Workspace override.
        scope: Scope,
        /// Optional opaque cursor.
        cursor: Option<String>,
    },
    /// Create one empty group.
    Create {
        /// Workspace override.
        scope: Scope,
        /// Human-readable group name.
        name: String,
    },
    /// Rename one active group.
    Rename {
        /// Workspace override.
        scope: Scope,
        /// Stable group identifier.
        group_id: String,
        /// Replacement name.
        name: String,
    },
    /// List current group members.
    ListMembers {
        /// Workspace override.
        scope: Scope,
        /// Stable group identifier.
        group_id: String,
        /// Optional opaque cursor.
        cursor: Option<String>,
    },
    /// Add one active local user to a group.
    AddMember {
        /// Workspace override.
        scope: Scope,
        /// Stable group identifier.
        group_id: String,
        /// Stable local-user identifier.
        user_id: String,
    },
    /// Remove one current group member.
    RemoveMember {
        /// Workspace override.
        scope: Scope,
        /// Stable group identifier.
        group_id: String,
        /// Stable local-user identifier.
        user_id: String,
        /// Explicit destructive confirmation.
        confirmed: bool,
    },
    /// Deactivate one group and revoke its grants.
    Deactivate {
        /// Workspace override.
        scope: Scope,
        /// Stable group identifier.
        group_id: String,
        /// Explicit destructive confirmation.
        confirmed: bool,
    },
}

pub(super) fn is_group_tool(name: &str) -> bool {
    matches!(
        name,
        "list_groups"
            | "create_group"
            | "rename_group"
            | "list_group_members"
            | "add_group_member"
            | "remove_group_member"
            | "deactivate_group"
    )
}

pub(super) fn parse_group_call(
    name: &str,
    arguments: &Map<String, Value>,
    scope: Scope,
) -> Result<GroupToolCall, String> {
    crate::reject_unknown_arguments(arguments, keys(name))?;
    match name {
        "list_groups" => Ok(GroupToolCall::List {
            scope,
            cursor: optional_string(arguments, "cursor")?,
        }),
        "create_group" => Ok(GroupToolCall::Create {
            scope,
            name: required_string(arguments, "name")?,
        }),
        "rename_group" => Ok(GroupToolCall::Rename {
            scope,
            group_id: required_string(arguments, "group_id")?,
            name: required_string(arguments, "name")?,
        }),
        "list_group_members" => Ok(GroupToolCall::ListMembers {
            scope,
            group_id: required_string(arguments, "group_id")?,
            cursor: optional_string(arguments, "cursor")?,
        }),
        "add_group_member" => Ok(GroupToolCall::AddMember {
            scope,
            group_id: required_string(arguments, "group_id")?,
            user_id: required_string(arguments, "user_id")?,
        }),
        "remove_group_member" => {
            require_confirmation(arguments)?;
            Ok(GroupToolCall::RemoveMember {
                scope,
                group_id: required_string(arguments, "group_id")?,
                user_id: required_string(arguments, "user_id")?,
                confirmed: true,
            })
        }
        "deactivate_group" => {
            require_confirmation(arguments)?;
            Ok(GroupToolCall::Deactivate {
                scope,
                group_id: required_string(arguments, "group_id")?,
                confirmed: true,
            })
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn keys(name: &str) -> &'static [&'static str] {
    match name {
        "list_groups" => &["cursor"],
        "create_group" => &["name"],
        "rename_group" => &["group_id", "name"],
        "list_group_members" => &["cursor", "group_id"],
        "add_group_member" => &["group_id", "user_id"],
        "remove_group_member" => &["confirm", "group_id", "user_id"],
        "deactivate_group" => &["confirm", "group_id"],
        _ => &[],
    }
}

#[cfg(test)]
#[path = "group_call_tests.rs"]
mod tests;
