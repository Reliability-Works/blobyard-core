use super::{WebYardToolCall, arguments::string_list};
use crate::{Scope, tool_call::required_string};
use serde_json::{Map, Value};

pub(super) fn is_tool(name: &str) -> bool {
    matches!(
        name,
        "list_yard_management_roles"
            | "set_yard_management_role"
            | "revoke_yard_management_role"
            | "get_yard_application_policy"
            | "set_yard_application_policy"
            | "set_yard_access_roles"
    )
}

pub(super) fn parse(
    name: &str,
    arguments: &Map<String, Value>,
    scope: Scope,
) -> Result<WebYardToolCall, String> {
    match name {
        "list_yard_management_roles" => Ok(WebYardToolCall::ListYardManagementRoles {
            scope,
            yard: required_string(arguments, "yard")?,
            cursor: crate::optional_string(arguments, "cursor")?,
        }),
        "set_yard_management_role" => Ok(WebYardToolCall::SetYardManagementRole {
            scope,
            yard: required_string(arguments, "yard")?,
            user_id: required_string(arguments, "user_id")?,
            role: required_string(arguments, "role")?,
        }),
        "revoke_yard_management_role" => Ok(WebYardToolCall::RevokeYardManagementRole {
            scope,
            yard: required_string(arguments, "yard")?,
            user_id: required_string(arguments, "user_id")?,
        }),
        "get_yard_application_policy" => Ok(WebYardToolCall::GetYardApplicationPolicy {
            scope,
            yard: required_string(arguments, "yard")?,
        }),
        "set_yard_application_policy" => Ok(WebYardToolCall::SetYardApplicationPolicy {
            scope,
            yard: required_string(arguments, "yard")?,
            source_manifest_digest: required_string(arguments, "source_manifest_digest")?,
            default_role: nullable_string(arguments, "default_role")?,
            roles: role_map(arguments, "roles")?,
        }),
        "set_yard_access_roles" => Ok(WebYardToolCall::SetYardAccessRoles {
            scope,
            yard: required_string(arguments, "yard")?,
            grant_id: required_string(arguments, "grant_id")?,
            roles: required_string_list(arguments, "roles")?,
        }),
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn required_string_list(arguments: &Map<String, Value>, key: &str) -> Result<Vec<String>, String> {
    if arguments.contains_key(key) {
        string_list(arguments, key)
    } else {
        Err(format!("missing required argument: {key}"))
    }
}

fn nullable_string(arguments: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match arguments.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value.clone())),
        Some(_) => Err(format!("{key} must be a non-empty string or null")),
        None => Err(format!("missing required argument: {key}")),
    }
}

fn role_map(arguments: &Map<String, Value>, key: &str) -> Result<Value, String> {
    arguments
        .get(key)
        .filter(|value| value.is_object())
        .cloned()
        .ok_or_else(|| format!("{key} must be an object"))
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::Scope;
    use serde_json::Map;

    #[test]
    fn rejects_unknown_internal_identity_tool() {
        assert_eq!(
            parse("unknown_identity_tool", &Map::new(), Scope::default()),
            Err("unknown tool: unknown_identity_tool".to_owned())
        );
    }
}
