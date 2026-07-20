#![allow(
    clippy::redundant_pub_crate,
    reason = "the private sibling tool-call parser dispatches to these Yard helpers"
)]

use crate::Scope;
use crate::tool_call::{optional_bool, required_string};
use serde_json::{Map, Value};

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
    match name {
        "deploy_web_yard" => parse_deploy(scope, arguments),
        "list_web_yards" => Ok(WebYardToolCall::ListWebYards { scope }),
        "list_yard_deploys" => Ok(WebYardToolCall::ListYardDeploys {
            scope,
            yard: required_string(arguments, "yard")?,
        }),
        "rollback_web_yard" => Ok(WebYardToolCall::RollbackWebYard {
            scope,
            yard: required_string(arguments, "yard")?,
            deploy_id: crate::optional_string(arguments, "deploy_id")?,
        }),
        "delete_web_yard" => {
            require_true(arguments, "confirm")?;
            Ok(WebYardToolCall::DeleteWebYard {
                scope,
                yard: required_string(arguments, "yard")?,
            })
        }
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
        "list_yard_deploys" => &["yard"],
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
