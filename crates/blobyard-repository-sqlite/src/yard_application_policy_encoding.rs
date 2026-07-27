use blobyard_core::{ApplicationPolicyGraph, EffectiveApplicationPolicy};
use std::collections::BTreeMap;

pub(super) fn role_json(roles: &[String]) -> String {
    string_array(roles)
}

pub(super) fn encode_graph(value: &ApplicationPolicyGraph) -> String {
    let default_role = value
        .default_role
        .as_ref()
        .map_or_else(|| "null".to_owned(), |role| quoted(role));
    let mut roles = String::new();
    for (name, definition) in &value.roles {
        push_separator(&mut roles);
        roles.push_str(&quoted(name));
        roles.push_str(":{\"inherits\":");
        roles.push_str(&string_array(&definition.inherits));
        roles.push_str(",\"permissions\":");
        roles.push_str(&string_array(&definition.permissions));
        roles.push('}');
    }
    format!(r#"{{"defaultRole":{default_role},"roles":{{{roles}}}}}"#)
}

pub(super) fn encode_effective(value: &EffectiveApplicationPolicy) -> String {
    format!(
        r#"{{"effectiveRoles":{},"effectivePermissions":{}}}"#,
        string_map(&value.effective_roles),
        string_map(&value.effective_permissions)
    )
}

fn string_map(values: &BTreeMap<String, Vec<String>>) -> String {
    let mut encoded = String::from("{");
    let mut entries = String::new();
    for (name, value) in values {
        push_separator(&mut entries);
        entries.push_str(&quoted(name));
        entries.push(':');
        entries.push_str(&string_array(value));
    }
    encoded.push_str(&entries);
    encoded.push('}');
    encoded
}

fn string_array(values: &[String]) -> String {
    let mut encoded = String::from("[");
    let mut entries = String::new();
    for value in values {
        push_separator(&mut entries);
        entries.push_str(&quoted(value));
    }
    encoded.push_str(&entries);
    encoded.push(']');
    encoded
}

fn quoted(value: &str) -> String {
    serde_json::Value::String(value.to_owned()).to_string()
}

fn push_separator(value: &mut String) {
    if !value.is_empty() {
        value.push(',');
    }
}
