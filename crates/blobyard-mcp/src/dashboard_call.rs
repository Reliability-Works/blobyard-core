#![allow(
    clippy::redundant_pub_crate,
    reason = "private sibling modules share the dashboard parser"
)]

use serde_json::{Map, Value};

use crate::tool_call::required_string;
use crate::{Scope, optional_string, reject_unknown_arguments};

/// A validated dashboard operation that is safe to expose through MCP.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DashboardToolCall {
    /// Show current billing, usage, and entitlements.
    GetBilling {
        /// Optional CLI scope overrides.
        scope: Scope,
    },
    /// Rename the selected workspace.
    RenameWorkspace {
        /// Optional CLI scope overrides.
        scope: Scope,
        /// Replacement workspace name.
        name: String,
    },
    /// Queue an account data export.
    RequestAccountExport {
        /// Optional CLI scope overrides.
        scope: Scope,
    },
    /// Show the current account export state.
    GetAccountExport {
        /// Optional CLI scope overrides.
        scope: Scope,
    },
    /// Show the current account deletion state.
    GetAccountDeletion {
        /// Optional CLI scope overrides.
        scope: Scope,
    },
    /// Show retention policy and execution state for the selected project.
    GetRetentionOverview {
        /// Optional CLI scope overrides.
        scope: Scope,
    },
}

pub(crate) fn is_dashboard_tool(name: &str) -> bool {
    matches!(
        name,
        "get_billing"
            | "rename_workspace"
            | "get_account_export"
            | "request_account_export"
            | "get_account_deletion"
            | "get_retention_overview"
    )
}

pub(crate) fn parse_dashboard_call(
    name: &str,
    arguments: &Map<String, Value>,
    scope: Scope,
) -> Result<DashboardToolCall, String> {
    reject_unknown(name, arguments)?;
    let call = match name {
        "get_billing" => Ok(DashboardToolCall::GetBilling { scope }),
        "rename_workspace" => Ok(DashboardToolCall::RenameWorkspace {
            scope,
            name: required_string(arguments, "name")?,
        }),
        "get_account_export" => Ok(DashboardToolCall::GetAccountExport { scope }),
        "request_account_export" => Ok(DashboardToolCall::RequestAccountExport { scope }),
        "get_account_deletion" => Ok(DashboardToolCall::GetAccountDeletion { scope }),
        "get_retention_overview" => Ok(DashboardToolCall::GetRetentionOverview { scope }),
        _ => Err(format!("unknown tool: {name}")),
    }?;
    Ok(call)
}

fn reject_unknown(name: &str, arguments: &Map<String, Value>) -> Result<(), String> {
    let specific: &[&str] = match name {
        "rename_workspace" => &["name"],
        _ => &[],
    };
    reject_unknown_arguments(arguments, specific)?;
    let _ = optional_string(arguments, "workspace")?;
    let _ = optional_string(arguments, "project")?;
    Ok(())
}
