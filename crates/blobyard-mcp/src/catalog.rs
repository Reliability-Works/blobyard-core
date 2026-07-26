use serde_json::{Map, Value};

use crate::catalog_access as access;
use crate::catalog_contracts as contracts;
use crate::catalog_contracts::{
    add, boolean, delete_contract, download_contract, inbox_contract, preview_contract,
    retention_contract, revoke_share_contract, scope_properties, share_contract, string,
    tool_schema, upload_contract,
};

#[path = "catalog_annotations.rs"]
mod annotations;
#[path = "group_catalog.rs"]
mod group_catalog;
#[path = "catalog_identity.rs"]
mod identity_catalog;
#[path = "catalog_read.rs"]
mod read_catalog;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum ToolKind {
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
    ListYardEnvironments,
    GetYardAccess,
    SetYardVisibility,
    GrantYardAccess,
    RevokeYardAccess,
    ListYardManagementRoles,
    SetYardManagementRole,
    RevokeYardManagementRole,
    GetYardApplicationPolicy,
    SetYardApplicationPolicy,
    SetYardAccessRoles,
    ListYardSessions,
    RevokeYardSession,
    RollbackWebYard,
    DeleteWebYard,
}

const TOOLS: [ToolKind; 39] = [
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
    ToolKind::ListYardEnvironments,
    ToolKind::GetYardAccess,
    ToolKind::SetYardVisibility,
    ToolKind::GrantYardAccess,
    ToolKind::RevokeYardAccess,
    ToolKind::ListYardManagementRoles,
    ToolKind::SetYardManagementRole,
    ToolKind::RevokeYardManagementRole,
    ToolKind::GetYardApplicationPolicy,
    ToolKind::SetYardApplicationPolicy,
    ToolKind::SetYardAccessRoles,
    ToolKind::ListYardSessions,
    ToolKind::RevokeYardSession,
    ToolKind::RollbackWebYard,
    ToolKind::DeleteWebYard,
];

pub(super) fn tools() -> Vec<Value> {
    TOOLS
        .into_iter()
        .map(tool)
        .chain(crate::dashboard_catalog::tools())
        .chain(crate::admin_catalog::tools())
        .chain(group_catalog::tools())
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
        &annotations::annotations(kind),
    )
}

fn tool_contract(kind: ToolKind) -> (&'static str, Map<String, Value>, Vec<&'static str>) {
    let mut properties = scope_properties();
    if let Some((description, required)) = identity_catalog::contract(kind, &mut properties) {
        return (description, properties, required);
    }
    if let Some((description, required)) = read_catalog::contract(kind) {
        return (description, properties, required);
    }
    let (description, required) = match kind {
        ToolKind::CreateWorkspace => named_resource_contract(
            &mut properties,
            "Human-readable workspace name.",
            "Create a workspace.",
        ),
        ToolKind::ListObjects => list_objects_contract(&mut properties),
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
        ToolKind::RevokePreview => identifier_contract(
            &mut properties,
            "preview_id",
            "Stable preview identifier.",
            "Revoke a static preview.",
        ),
        ToolKind::CreateInbox => inbox_contract(&mut properties),
        ToolKind::RevokeInbox => identifier_contract(
            &mut properties,
            "inbox_id",
            "Stable inbox identifier.",
            "Revoke an upload inbox.",
        ),
        ToolKind::SetRetention => retention_contract(&mut properties),
        ToolKind::ClearRetention => ("Clear the selected project's retention policy.", vec![]),
        ToolKind::DeployWebYard => contracts::deploy_yard_contract(&mut properties),
        ToolKind::ListYardDeploys => contracts::list_yard_deploys_contract(&mut properties),
        ToolKind::ListYardEnvironments => {
            contracts::list_yard_environments_contract(&mut properties)
        }
        ToolKind::GetYardAccess => access::yard_access_contract(&mut properties),
        ToolKind::SetYardVisibility => access::set_yard_visibility_contract(&mut properties),
        ToolKind::GrantYardAccess => access::grant_yard_access_contract(&mut properties),
        ToolKind::RevokeYardAccess => access::revoke_yard_access_contract(&mut properties),
        ToolKind::ListYardSessions => access::list_yard_sessions_contract(&mut properties),
        ToolKind::RevokeYardSession => access::revoke_yard_session_contract(&mut properties),
        ToolKind::RollbackWebYard => contracts::rollback_yard_contract(&mut properties),
        ToolKind::DeleteWebYard => contracts::delete_yard_contract(&mut properties),
        _ => unreachable!("delegated catalog contract"),
    };
    (description, properties, required)
}

fn identifier_contract(
    properties: &mut Map<String, Value>,
    key: &'static str,
    field_description: &'static str,
    description: &'static str,
) -> (&'static str, Vec<&'static str>) {
    add(properties, key, string(field_description));
    (description, vec![key])
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

impl ToolKind {
    pub(super) const fn name(self) -> &'static str {
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
            Self::ListYardEnvironments => "list_yard_environments",
            Self::GetYardAccess => "get_yard_access",
            Self::SetYardVisibility => "set_yard_visibility",
            Self::GrantYardAccess => "grant_yard_access",
            Self::RevokeYardAccess => "revoke_yard_access",
            Self::ListYardManagementRoles => "list_yard_management_roles",
            Self::SetYardManagementRole => "set_yard_management_role",
            Self::RevokeYardManagementRole => "revoke_yard_management_role",
            Self::GetYardApplicationPolicy => "get_yard_application_policy",
            Self::SetYardApplicationPolicy => "set_yard_application_policy",
            Self::SetYardAccessRoles => "set_yard_access_roles",
            Self::ListYardSessions => "list_yard_sessions",
            Self::RevokeYardSession => "revoke_yard_session",
            Self::RollbackWebYard => "rollback_web_yard",
            Self::DeleteWebYard => "delete_web_yard",
        }
    }
}
