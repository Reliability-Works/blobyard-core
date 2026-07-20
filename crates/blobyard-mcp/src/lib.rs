//! Private MCP protocol adapter for Blobyard's authorized CLI operations.

use serde_json::{Map, Value};

pub(crate) mod admin_call;
pub(crate) mod admin_catalog;
pub(crate) mod catalog_contracts;
mod dashboard_call;
mod dashboard_catalog;
mod protocol;
mod server;
pub(crate) mod tool_call;
mod yard_call;

pub use admin_call::AdminToolCall;
pub use dashboard_call::DashboardToolCall;
pub use protocol::{BackendError, BackendFuture, ToolBackend};
pub use server::{McpServer, serve, serve_stdio};
pub use tool_call::{Scope, ToolCall};
pub use yard_call::WebYardToolCall;

fn optional_string(arguments: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    arguments
        .get(key)
        .map(|value| {
            value
                .as_str()
                .filter(|text| !text.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| format!("{key} must be a non-empty string"))
        })
        .transpose()
}

fn reject_unknown_arguments(
    arguments: &Map<String, Value>,
    specific: &[&str],
) -> Result<(), String> {
    if let Some(key) = arguments.keys().find(|key| {
        !matches!(key.as_str(), "workspace" | "project") && !specific.contains(&key.as_str())
    }) {
        return Err(format!("unexpected argument: {key}"));
    }
    Ok(())
}

#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod tool_call_tests;
