use serde_json::{Map, Value, json};

use crate::catalog_contracts::{
    add, boolean, delete_contract, download_contract, inbox_contract, preview_contract,
    retention_contract, revoke_share_contract, scope_properties, share_contract, string, title,
    tool_schema, upload_contract,
};

#[derive(Clone, Copy, Eq, PartialEq)]
enum ToolKind {
    Whoami,
    ListWorkspaces,
    CreateWorkspace,
    ListProjects,
    ListObjects,
    GetRetention,
    ListInboxes,
    ListShares,
    ListPreviews,
    CreateProject,
    UploadFile,
    DownloadFile,
    DeleteObject,
    CreateShare,
    RevokeShare,
    CreatePreview,
    RevokePreview,
    CreateInbox,
    RevokeInbox,
    SetRetention,
    ClearRetention,
    DeployWebYard,
    ListWebYards,
    ListYardDeploys,
    RollbackWebYard,
    DeleteWebYard,
}

const TOOLS: [ToolKind; 26] = [
    ToolKind::Whoami,
    ToolKind::ListWorkspaces,
    ToolKind::CreateWorkspace,
    ToolKind::ListProjects,
    ToolKind::ListObjects,
    ToolKind::GetRetention,
    ToolKind::ListInboxes,
    ToolKind::ListShares,
    ToolKind::ListPreviews,
    ToolKind::CreateProject,
    ToolKind::UploadFile,
    ToolKind::DownloadFile,
    ToolKind::DeleteObject,
    ToolKind::CreateShare,
    ToolKind::RevokeShare,
    ToolKind::CreatePreview,
    ToolKind::RevokePreview,
    ToolKind::CreateInbox,
    ToolKind::RevokeInbox,
    ToolKind::SetRetention,
    ToolKind::ClearRetention,
    ToolKind::DeployWebYard,
    ToolKind::ListWebYards,
    ToolKind::ListYardDeploys,
    ToolKind::RollbackWebYard,
    ToolKind::DeleteWebYard,
];

pub(super) fn tools() -> Vec<Value> {
    TOOLS
        .into_iter()
        .map(tool)
        .chain(crate::dashboard_catalog::tools())
        .chain(crate::admin_catalog::tools())
        .collect()
}

fn tool(kind: ToolKind) -> Value {
    let name = kind.name();
    let (description, properties, required) = tool_contract(kind);
    tool_schema(
        name,
        description,
        &properties,
        &required,
        &annotations(kind),
    )
}

fn tool_contract(kind: ToolKind) -> (&'static str, Map<String, Value>, Vec<&'static str>) {
    let mut properties = scope_properties();
    let (description, required) = match kind {
        ToolKind::Whoami => (
            "Show the authenticated Blobyard identity and selected scope.",
            vec![],
        ),
        ToolKind::ListWorkspaces => ("List workspaces visible to the current identity.", vec![]),
        ToolKind::CreateWorkspace => named_resource_contract(
            &mut properties,
            "Human-readable workspace name.",
            "Create a workspace.",
        ),
        ToolKind::ListProjects => ("List projects visible in the selected workspace.", vec![]),
        ToolKind::ListObjects => list_objects_contract(&mut properties),
        ToolKind::GetRetention => ("Show the selected project's retention policy.", vec![]),
        ToolKind::ListInboxes => ("List redacted inboxes in the selected project.", vec![]),
        ToolKind::ListShares => ("List redacted shares in the selected workspace.", vec![]),
        ToolKind::ListPreviews => ("List redacted previews in the selected project.", vec![]),
        ToolKind::CreateProject => named_resource_contract(
            &mut properties,
            "Human-readable project name.",
            "Create a project in the selected workspace.",
        ),
        ToolKind::UploadFile => upload_contract(&mut properties),
        ToolKind::DownloadFile => download_contract(&mut properties),
        ToolKind::DeleteObject => delete_contract(&mut properties),
        ToolKind::CreateShare => share_contract(&mut properties),
        ToolKind::RevokeShare => revoke_share_contract(&mut properties),
        ToolKind::CreatePreview => preview_contract(&mut properties),
        ToolKind::RevokePreview => {
            add(
                &mut properties,
                "preview_id",
                string("Stable preview identifier."),
            );
            ("Revoke a static preview.", vec!["preview_id"])
        }
        ToolKind::CreateInbox => inbox_contract(&mut properties),
        ToolKind::RevokeInbox => {
            add(
                &mut properties,
                "inbox_id",
                string("Stable inbox identifier."),
            );
            ("Revoke an upload inbox.", vec!["inbox_id"])
        }
        ToolKind::SetRetention => retention_contract(&mut properties),
        ToolKind::ClearRetention => ("Clear the selected project's retention policy.", vec![]),
        ToolKind::DeployWebYard => crate::catalog_contracts::deploy_yard_contract(&mut properties),
        ToolKind::ListWebYards => ("List Web Yards in the selected project.", vec![]),
        ToolKind::ListYardDeploys => {
            crate::catalog_contracts::list_yard_deploys_contract(&mut properties)
        }
        ToolKind::RollbackWebYard => {
            crate::catalog_contracts::rollback_yard_contract(&mut properties)
        }
        ToolKind::DeleteWebYard => crate::catalog_contracts::delete_yard_contract(&mut properties),
    };
    (description, properties, required)
}

fn named_resource_contract(
    properties: &mut Map<String, Value>,
    name_description: &'static str,
    description: &'static str,
) -> (&'static str, Vec<&'static str>) {
    add(properties, "name", string(name_description));
    (description, vec!["name"])
}

fn list_objects_contract(properties: &mut Map<String, Value>) -> (&'static str, Vec<&'static str>) {
    add(
        properties,
        "prefix",
        string("Optional blobyard:// URI prefix."),
    );
    add(
        properties,
        "versions",
        boolean("Include immutable versions."),
    );
    (
        "List objects under an optional Blobyard URI prefix.",
        vec![],
    )
}

fn annotations(kind: ToolKind) -> Value {
    let name = kind.name();
    let read_only = matches!(
        kind,
        ToolKind::Whoami
            | ToolKind::ListWorkspaces
            | ToolKind::ListProjects
            | ToolKind::ListObjects
            | ToolKind::GetRetention
            | ToolKind::ListInboxes
            | ToolKind::ListShares
            | ToolKind::ListPreviews
            | ToolKind::ListWebYards
            | ToolKind::ListYardDeploys
    );
    let destructive = matches!(
        kind,
        ToolKind::DeleteObject
            | ToolKind::RevokeShare
            | ToolKind::RevokePreview
            | ToolKind::RevokeInbox
            | ToolKind::SetRetention
            | ToolKind::ClearRetention
            | ToolKind::RollbackWebYard
            | ToolKind::DeleteWebYard
    );
    let idempotent = read_only || destructive || kind == ToolKind::DownloadFile;
    let open_world = matches!(
        kind,
        ToolKind::CreateShare
            | ToolKind::CreatePreview
            | ToolKind::CreateInbox
            | ToolKind::DeployWebYard
    );
    json!({
        "title": title(name),
        "readOnlyHint": read_only,
        "destructiveHint": destructive,
        "idempotentHint": idempotent,
        "openWorldHint": open_world
    })
}

impl ToolKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Whoami => "whoami",
            Self::ListWorkspaces => "list_workspaces",
            Self::CreateWorkspace => "create_workspace",
            Self::ListProjects => "list_projects",
            Self::ListObjects => "list_objects",
            Self::GetRetention => "get_retention",
            Self::ListInboxes => "list_inboxes",
            Self::ListShares => "list_shares",
            Self::ListPreviews => "list_previews",
            Self::CreateProject => "create_project",
            Self::UploadFile => "upload_file",
            Self::DownloadFile => "download_file",
            Self::DeleteObject => "delete_object",
            Self::CreateShare => "create_share",
            Self::RevokeShare => "revoke_share",
            Self::CreatePreview => "create_preview",
            Self::RevokePreview => "revoke_preview",
            Self::CreateInbox => "create_inbox",
            Self::RevokeInbox => "revoke_inbox",
            Self::SetRetention => "set_retention",
            Self::ClearRetention => "clear_retention",
            Self::DeployWebYard => "deploy_web_yard",
            Self::ListWebYards => "list_web_yards",
            Self::ListYardDeploys => "list_yard_deploys",
            Self::RollbackWebYard => "rollback_web_yard",
            Self::DeleteWebYard => "delete_web_yard",
        }
    }
}
