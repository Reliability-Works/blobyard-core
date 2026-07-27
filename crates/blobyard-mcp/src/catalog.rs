use serde_json::{Map, Value};

use crate::catalog_access as access;
use crate::catalog_access::{
    create_yard_guest_invite_contract as create_guest_invite,
    get_yard_application_policy_contract as get_application_policy,
    list_yard_guest_invites_contract as list_guest_invites,
    list_yard_management_roles_contract as list_management_roles,
    revoke_yard_guest_invite_contract as revoke_guest_invite,
    revoke_yard_management_role_contract as revoke_management_role,
    set_yard_access_roles_contract as set_access_roles,
    set_yard_application_policy_contract as set_application_policy,
    set_yard_management_role_contract as set_management_role,
};
use crate::catalog_contracts as contracts;
use crate::catalog_contracts::{
    add, boolean, delete_contract, download_contract, inbox_contract,
    list_yard_environments_contract as list_yard_environments, preview_contract,
    retention_contract, revoke_share_contract, scope_properties, share_contract, string,
    tool_schema, upload_contract,
};

#[path = "catalog_annotations.rs"]
mod annotations;
#[path = "group_catalog.rs"]
mod group_catalog;
#[path = "catalog_names.rs"]
mod names;

const WORKSPACE_NAME: &str = "Human-readable workspace name.";
const PROJECT_NAME: &str = "Human-readable project name.";
const CREATE_WORKSPACE: &str = "Create a workspace.";
const CREATE_PROJECT: &str = "Create a project in the selected workspace.";

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
    ListYardGuestInvites,
    CreateYardGuestInvite,
    RevokeYardGuestInvite,
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

const TOOLS: [ToolKind; 42] = [
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
    ToolKind::ListYardGuestInvites,
    ToolKind::CreateYardGuestInvite,
    ToolKind::RevokeYardGuestInvite,
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
    let (description, required) = match kind {
        ToolKind::Whoami => (
            "Show the authenticated Blobyard identity and selected scope.",
            vec![],
        ),
        ToolKind::ListWorkspaces => ("List workspaces visible to the current identity.", vec![]),
        ToolKind::ListProjects => ("List projects visible in the selected workspace.", vec![]),
        ToolKind::GetRetention => ("Show the selected project's retention policy.", vec![]),
        ToolKind::ListInboxes => ("List redacted inboxes in the selected project.", vec![]),
        ToolKind::ListShares => ("List redacted shares in the selected workspace.", vec![]),
        ToolKind::ListPreviews => ("List redacted previews in the selected project.", vec![]),
        ToolKind::ListWebYards => ("List Web Yards in the selected project.", vec![]),
        ToolKind::CreateWorkspace => create_workspace_contract(&mut properties),
        ToolKind::ListObjects => list_objects_contract(&mut properties),
        ToolKind::CreateProject => create_project_contract(&mut properties),
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
        ToolKind::ListYardEnvironments => list_yard_environments(&mut properties),
        ToolKind::GetYardAccess => access::yard_access_contract(&mut properties),
        ToolKind::SetYardVisibility => access::set_yard_visibility_contract(&mut properties),
        ToolKind::GrantYardAccess => access::grant_yard_access_contract(&mut properties),
        ToolKind::RevokeYardAccess => access::revoke_yard_access_contract(&mut properties),
        ToolKind::ListYardGuestInvites => list_guest_invites(&mut properties),
        ToolKind::CreateYardGuestInvite => create_guest_invite(&mut properties),
        ToolKind::RevokeYardGuestInvite => revoke_guest_invite(&mut properties),
        ToolKind::ListYardManagementRoles => list_management_roles(&mut properties),
        ToolKind::SetYardManagementRole => set_management_role(&mut properties),
        ToolKind::RevokeYardManagementRole => revoke_management_role(&mut properties),
        ToolKind::GetYardApplicationPolicy => get_application_policy(&mut properties),
        ToolKind::SetYardApplicationPolicy => set_application_policy(&mut properties),
        ToolKind::SetYardAccessRoles => set_access_roles(&mut properties),
        ToolKind::ListYardSessions => access::list_yard_sessions_contract(&mut properties),
        ToolKind::RevokeYardSession => access::revoke_yard_session_contract(&mut properties),
        ToolKind::RollbackWebYard => contracts::rollback_yard_contract(&mut properties),
        ToolKind::DeleteWebYard => contracts::delete_yard_contract(&mut properties),
    };
    (description, properties, required)
}

fn create_workspace_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    named_resource_contract(properties, WORKSPACE_NAME, CREATE_WORKSPACE)
}

fn create_project_contract(
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    named_resource_contract(properties, PROJECT_NAME, CREATE_PROJECT)
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
