use super::ToolKind;
use crate::catalog_contracts::title;
use serde_json::{Value, json};

pub(super) fn annotations(kind: ToolKind) -> Value {
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
            | ToolKind::ListYardEnvironments
            | ToolKind::GetYardAccess
            | ToolKind::ListYardManagementRoles
            | ToolKind::GetYardApplicationPolicy
            | ToolKind::ListYardSessions
    );
    let destructive = matches!(
        kind,
        ToolKind::DeleteObject
            | ToolKind::RevokeShare
            | ToolKind::RevokePreview
            | ToolKind::RevokeInbox
            | ToolKind::SetRetention
            | ToolKind::ClearRetention
            | ToolKind::SetYardVisibility
            | ToolKind::RevokeYardAccess
            | ToolKind::RevokeYardManagementRole
            | ToolKind::RevokeYardSession
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
            | ToolKind::GrantYardAccess
    );
    json!({
        "title": title(name),
        "readOnlyHint": read_only,
        "destructiveHint": destructive,
        "idempotentHint": idempotent,
        "openWorldHint": open_world
    })
}
