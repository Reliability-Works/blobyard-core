use super::WebYardToolCall;
use crate::Scope;
use crate::tool_call::{optional_bool, required_string};
use serde_json::{Map, Value};

pub(super) fn parse_deploy(
    scope: Scope,
    arguments: &Map<String, Value>,
) -> Result<WebYardToolCall, String> {
    require_true(arguments, "public")?;
    Ok(WebYardToolCall::DeployWebYard {
        scope,
        directory: required_string(arguments, "directory")?,
        yard: required_string(arguments, "yard")?,
        spa: optional_bool(arguments, "spa")?.unwrap_or(false),
        clean_urls: optional_bool(arguments, "clean_urls")?.unwrap_or(false),
    })
}

pub(super) fn parse_delete(
    scope: Scope,
    arguments: &Map<String, Value>,
) -> Result<WebYardToolCall, String> {
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

pub(super) fn reject_unknown(name: &str, arguments: &Map<String, Value>) -> Result<(), String> {
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
    crate::reject_unknown_arguments(arguments, specific)
}
