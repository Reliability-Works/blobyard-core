use crate::Scope;
use crate::tool_call::required_string;
use serde_json::{Map, Value};

/// A validated MCP operation for Web Yard guest invitations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum YardGuestInviteToolCall {
    /// List one invitation page without secret material.
    List {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
        /// Optional opaque continuation cursor.
        cursor: Option<String>,
    },
    /// Create one invitation and return its raw URL once.
    Create {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
        /// Normalized guest email.
        email: String,
        /// Application roles granted to the guest.
        roles: Vec<String>,
        /// Optional environment restriction.
        environment_id: Option<String>,
        /// Optional RFC 3339 expiry; omitted defaults to seven days.
        expires_at: Option<String>,
    },
    /// Revoke one guest invitation.
    Revoke {
        /// CLI scope overrides.
        scope: Scope,
        /// Project-unique Web Yard name.
        yard: String,
        /// Stable invitation identifier.
        invitation_id: String,
    },
}

pub(super) fn is_tool(name: &str) -> bool {
    matches!(
        name,
        "list_yard_guest_invites" | "create_yard_guest_invite" | "revoke_yard_guest_invite"
    )
}

pub(super) fn parse(
    name: &str,
    arguments: &Map<String, Value>,
    scope: Scope,
) -> Result<YardGuestInviteToolCall, String> {
    reject_unknown(name, arguments)?;
    match name {
        "list_yard_guest_invites" => Ok(YardGuestInviteToolCall::List {
            scope,
            yard: required_string(arguments, "yard")?,
            cursor: crate::optional_string(arguments, "cursor")?,
        }),
        "create_yard_guest_invite" => Ok(YardGuestInviteToolCall::Create {
            scope,
            yard: required_string(arguments, "yard")?,
            email: required_string(arguments, "email")?,
            roles: super::arguments::string_list(arguments, "roles")?,
            environment_id: crate::optional_string(arguments, "environment_id")?,
            expires_at: crate::optional_string(arguments, "expires_at")?,
        }),
        "revoke_yard_guest_invite" => Ok(YardGuestInviteToolCall::Revoke {
            scope,
            yard: required_string(arguments, "yard")?,
            invitation_id: required_string(arguments, "invitation_id")?,
        }),
        _ => crate::unknown_tool(name),
    }
}

fn reject_unknown(name: &str, arguments: &Map<String, Value>) -> Result<(), String> {
    let specific: &[&str] = match name {
        "list_yard_guest_invites" => &["yard", "cursor"],
        "create_yard_guest_invite" => &["yard", "email", "roles", "environment_id", "expires_at"],
        "revoke_yard_guest_invite" => &["yard", "invitation_id"],
        _ => &[],
    };
    crate::reject_unknown_arguments(arguments, specific)
}

#[cfg(test)]
mod tests {
    use crate::Scope;
    use serde_json::Map;

    #[test]
    fn unknown_guest_invitation_tools_fail_closed() {
        assert_eq!(
            super::parse("unknown_guest_invite", &Map::new(), Scope::default()),
            Err("unknown tool: unknown_guest_invite".to_owned())
        );
    }
}
