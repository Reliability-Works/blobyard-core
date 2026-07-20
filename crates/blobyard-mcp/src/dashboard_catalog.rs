#![allow(
    clippy::redundant_pub_crate,
    reason = "the private root catalog composes this dashboard catalog"
)]

use serde_json::{Map, Value, json};

use crate::catalog_contracts::{add, scope_properties, string, title, tool_schema};

const TOOLS: [&str; 6] = [
    "get_billing",
    "rename_workspace",
    "get_account_export",
    "request_account_export",
    "get_account_deletion",
    "get_retention_overview",
];

pub(super) fn tools() -> impl Iterator<Item = Value> {
    TOOLS.into_iter().map(tool)
}

fn tool(name: &'static str) -> Value {
    let mut properties = scope_properties();
    let (description, required) = contract(name, &mut properties);
    tool_schema(
        name,
        description,
        &properties,
        &required,
        &annotations(name),
    )
}

fn contract(
    name: &'static str,
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    match name {
        "get_billing" => ("Show current billing, usage, and entitlements.", vec![]),
        "rename_workspace" => {
            add(properties, "name", string("Replacement workspace name."));
            ("Rename the selected workspace.", vec!["name"])
        }
        "get_account_export" => ("Show the current account export state.", vec![]),
        "request_account_export" => ("Queue a portable account data export.", vec![]),
        "get_account_deletion" => ("Show the current account deletion state.", vec![]),
        "get_retention_overview" => (
            "Show retention policy and execution state for the selected project.",
            vec![],
        ),
        _ => ("", vec![]),
    }
}

fn annotations(name: &str) -> Value {
    let read_only = matches!(
        name,
        "get_billing" | "get_account_export" | "get_account_deletion" | "get_retention_overview"
    );
    json!({
        "title": title(name),
        "readOnlyHint": read_only,
        "destructiveHint": false,
        "idempotentHint": read_only || name == "rename_workspace",
        "openWorldHint": false
    })
}

#[cfg(test)]
mod tests {
    use super::contract;
    use serde_json::Map;

    #[test]
    fn unknown_dashboard_contract_is_inert() {
        let mut properties = Map::new();
        let (description, required) = contract("unknown", &mut properties);
        assert!(description.is_empty());
        assert!(required.is_empty());
        assert!(properties.is_empty());
    }
}
