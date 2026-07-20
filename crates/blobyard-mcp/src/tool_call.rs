#![allow(
    clippy::redundant_pub_crate,
    reason = "the private sibling Yard parser shares these scalar validators"
)]

use serde_json::{Map, Value};

use crate::{
    admin_call::{AdminToolCall, is_admin_tool, parse_admin_call},
    optional_string, reject_unknown_arguments,
};

/// Optional CLI scope overrides shared by Blobyard tools.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Scope {
    /// Workspace slug override.
    pub workspace: Option<String>,
    /// Project slug override.
    pub project: Option<String>,
}

/// A validated call that the host can map directly to Blobyard CLI commands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolCall {
    /// Perform an account or workspace administration operation.
    Admin(AdminToolCall),
    /// Perform a safe dashboard or account-lifecycle operation.
    Dashboard(crate::DashboardToolCall),
    /// Perform a Web Yard deployment or management operation.
    WebYard(crate::WebYardToolCall),
    /// Show the current identity and selected scope.
    Whoami {
        /// CLI scope overrides.
        scope: Scope,
    },
    /// List workspaces visible to the current identity.
    ListWorkspaces {
        /// Optional CLI scope overrides retained for a uniform tool contract.
        scope: Scope,
    },
    /// Create a workspace.
    CreateWorkspace {
        /// Optional CLI scope overrides retained for a uniform tool contract.
        scope: Scope,
        /// Human-readable workspace name.
        name: String,
    },
    /// List projects in the selected workspace.
    ListProjects {
        /// CLI scope overrides.
        scope: Scope,
    },
    /// List objects under an optional URI prefix.
    ListObjects {
        /// CLI scope overrides.
        scope: Scope,
        /// Optional Blobyard URI prefix.
        prefix: Option<String>,
        /// Whether immutable versions are included.
        versions: bool,
    },
    /// Show the selected project's retention policy.
    GetRetention {
        /// CLI scope overrides.
        scope: Scope,
    },
    /// List upload inboxes for the selected project.
    ListInboxes {
        /// CLI scope overrides.
        scope: Scope,
    },
    /// List redacted shares in the selected workspace.
    ListShares {
        /// CLI scope overrides.
        scope: Scope,
    },
    /// List redacted previews in the selected project.
    ListPreviews {
        /// CLI scope overrides.
        scope: Scope,
    },
    /// Create a project.
    CreateProject {
        /// CLI scope overrides.
        scope: Scope,
        /// Human-readable project name.
        name: String,
    },
    /// Upload a local file or directory.
    UploadFile {
        /// CLI scope overrides.
        scope: Scope,
        /// Local file or directory path.
        source: String,
        /// Optional destination logical path.
        path: Option<String>,
        /// Whether ignored files are included.
        include_ignored: bool,
    },
    /// Download an object to a local path.
    DownloadFile {
        /// CLI scope overrides.
        scope: Scope,
        /// Blobyard URI to download.
        uri: String,
        /// Local destination path.
        output: String,
        /// Whether an existing destination may be replaced.
        force: bool,
    },
    /// Delete a logical object.
    DeleteObject {
        /// CLI scope overrides.
        scope: Scope,
        /// Blobyard URI to delete.
        uri: String,
    },
    /// Create an expiring share.
    CreateShare {
        /// CLI scope overrides.
        scope: Scope,
        /// Local path or Blobyard URI to share.
        target: String,
        /// Optional share lifetime.
        expires: Option<String>,
        /// Optional recipient email address.
        notify: Option<String>,
    },
    /// Revoke an existing public share.
    RevokeShare {
        /// CLI scope overrides.
        scope: Scope,
        /// Stable share identifier.
        share_id: String,
    },
    /// Publish an isolated static preview.
    CreatePreview {
        /// CLI scope overrides.
        scope: Scope,
        /// Local static directory.
        directory: String,
        /// Optional preview lifetime.
        expires: Option<String>,
    },
    /// Revoke an existing static preview.
    RevokePreview {
        /// CLI scope overrides.
        scope: Scope,
        /// Stable preview identifier.
        preview_id: String,
    },
    /// Create a guest upload inbox.
    CreateInbox {
        /// CLI scope overrides.
        scope: Scope,
        /// Human-readable inbox name.
        name: String,
        /// Optional inbox lifetime.
        expires: Option<String>,
    },
    /// Revoke an upload inbox.
    RevokeInbox {
        /// CLI scope overrides.
        scope: Scope,
        /// Stable inbox identifier.
        inbox_id: String,
    },
    /// Set or replace a retention policy.
    SetRetention {
        /// CLI scope overrides.
        scope: Scope,
        /// Number of matching versions to retain.
        latest: u32,
        /// Optional branch glob.
        branch: Option<String>,
        /// Optional logical path glob.
        path: Option<String>,
    },
    /// Clear the selected project's retention policy.
    ClearRetention {
        /// CLI scope overrides.
        scope: Scope,
    },
}

impl ToolCall {
    pub(crate) fn parse(name: &str, value: &Value) -> Result<Self, String> {
        let name = name
            .strip_prefix("blobyard_")
            .ok_or_else(|| format!("unknown tool: {name}"))?;
        let arguments = value
            .as_object()
            .ok_or_else(|| "tool arguments must be an object".to_owned())?;
        let scope = parse_scope(arguments)?;
        if is_admin_tool(name) {
            return parse_admin_call(name, arguments, scope).map(Self::Admin);
        }
        if crate::dashboard_call::is_dashboard_tool(name) {
            return crate::dashboard_call::parse_dashboard_call(name, arguments, scope)
                .map(Self::Dashboard);
        }
        if crate::yard_call::is_yard_tool(name) {
            return crate::yard_call::parse_yard_call(name, arguments, scope).map(Self::WebYard);
        }
        parse_regular(name, arguments, scope)
    }
}

fn parse_regular(
    name: &str,
    arguments: &Map<String, Value>,
    scope: Scope,
) -> Result<ToolCall, String> {
    reject_unknown(name, arguments)?;
    match name {
        "whoami" => Ok(ToolCall::Whoami { scope }),
        "list_workspaces" => Ok(ToolCall::ListWorkspaces { scope }),
        "create_workspace" => Ok(ToolCall::CreateWorkspace {
            scope,
            name: required_string(arguments, "name")?,
        }),
        "list_projects" => Ok(ToolCall::ListProjects { scope }),
        "list_objects" => Ok(ToolCall::ListObjects {
            scope,
            prefix: optional_string(arguments, "prefix")?,
            versions: optional_bool(arguments, "versions")?.unwrap_or(false),
        }),
        "get_retention" => Ok(ToolCall::GetRetention { scope }),
        "list_inboxes" => Ok(ToolCall::ListInboxes { scope }),
        "list_shares" => Ok(ToolCall::ListShares { scope }),
        "list_previews" => Ok(ToolCall::ListPreviews { scope }),
        "create_project" => Ok(ToolCall::CreateProject {
            scope,
            name: required_string(arguments, "name")?,
        }),
        "upload_file" => parse_upload(scope, arguments),
        "download_file" => parse_download(scope, arguments),
        "delete_object" => Ok(ToolCall::DeleteObject {
            scope,
            uri: required_string(arguments, "uri")?,
        }),
        "create_share" => parse_share(scope, arguments),
        "revoke_share" => Ok(ToolCall::RevokeShare {
            scope,
            share_id: required_string(arguments, "share_id")?,
        }),
        "create_preview" => Ok(ToolCall::CreatePreview {
            scope,
            directory: required_string(arguments, "directory")?,
            expires: optional_string(arguments, "expires")?,
        }),
        "revoke_preview" => Ok(ToolCall::RevokePreview {
            scope,
            preview_id: required_string(arguments, "preview_id")?,
        }),
        "create_inbox" => Ok(ToolCall::CreateInbox {
            scope,
            name: required_string(arguments, "name")?,
            expires: optional_string(arguments, "expires")?,
        }),
        "revoke_inbox" => Ok(ToolCall::RevokeInbox {
            scope,
            inbox_id: required_string(arguments, "inbox_id")?,
        }),
        "set_retention" => parse_retention(scope, arguments),
        "clear_retention" => Ok(ToolCall::ClearRetention { scope }),
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn reject_unknown(name: &str, arguments: &Map<String, Value>) -> Result<(), String> {
    let specific: &[&str] = match name {
        "create_workspace" | "create_project" => &["name"],
        "list_objects" => &["prefix", "versions"],
        "upload_file" => &["source", "path", "include_ignored"],
        "download_file" => &["uri", "output", "force"],
        "delete_object" => &["uri"],
        "create_share" => &["target", "expires", "notify"],
        "revoke_share" => &["share_id"],
        "create_preview" => &["directory", "expires"],
        "revoke_preview" => &["preview_id"],
        "create_inbox" => &["name", "expires"],
        "revoke_inbox" => &["inbox_id"],
        "set_retention" => &["latest", "branch", "path"],
        _ => &[],
    };
    reject_unknown_arguments(arguments, specific)
}

fn parse_scope(arguments: &Map<String, Value>) -> Result<Scope, String> {
    Ok(Scope {
        workspace: optional_string(arguments, "workspace")?,
        project: optional_string(arguments, "project")?,
    })
}

fn parse_upload(scope: Scope, arguments: &Map<String, Value>) -> Result<ToolCall, String> {
    Ok(ToolCall::UploadFile {
        scope,
        source: required_string(arguments, "source")?,
        path: optional_string(arguments, "path")?,
        include_ignored: optional_bool(arguments, "include_ignored")?.unwrap_or(false),
    })
}

fn parse_download(scope: Scope, arguments: &Map<String, Value>) -> Result<ToolCall, String> {
    Ok(ToolCall::DownloadFile {
        scope,
        uri: required_string(arguments, "uri")?,
        output: required_string(arguments, "output")?,
        force: optional_bool(arguments, "force")?.unwrap_or(false),
    })
}

fn parse_share(scope: Scope, arguments: &Map<String, Value>) -> Result<ToolCall, String> {
    Ok(ToolCall::CreateShare {
        scope,
        target: required_string(arguments, "target")?,
        expires: optional_string(arguments, "expires")?,
        notify: optional_string(arguments, "notify")?,
    })
}

fn parse_retention(scope: Scope, arguments: &Map<String, Value>) -> Result<ToolCall, String> {
    let latest = arguments
        .get("latest")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| "latest must be a positive 32-bit integer".to_owned())?;
    Ok(ToolCall::SetRetention {
        scope,
        latest,
        branch: optional_string(arguments, "branch")?,
        path: optional_string(arguments, "path")?,
    })
}

pub(crate) fn required_string(arguments: &Map<String, Value>, key: &str) -> Result<String, String> {
    optional_string(arguments, key)?.ok_or_else(|| format!("missing required argument: {key}"))
}

pub(crate) fn optional_bool(
    arguments: &Map<String, Value>,
    key: &str,
) -> Result<Option<bool>, String> {
    arguments
        .get(key)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| format!("{key} must be a boolean"))
        })
        .transpose()
}
