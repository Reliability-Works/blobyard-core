use serde_json::{Map, Value, json};

use crate::catalog_contracts::{add, boolean, scope_properties, string, title, tool_schema};

#[derive(Clone, Copy)]
enum Kind {
    ListAudit,
    ListMembers,
    ListInvites,
    CreateInvite,
    RevokeInvite,
    UpdateMemberRole,
    RemoveMember,
    ListApiTokens,
    RevokeApiToken,
    ListCiTrusts,
    CreateCiTrust,
    RevokeCiTrust,
    ListCliSessions,
    RevokeCliSession,
}

#[derive(Clone, Copy)]
enum WriteKind {
    CreateInvite,
    RevokeInvite,
    UpdateMemberRole,
    RemoveMember,
    RevokeApiToken,
    CreateCiTrust,
    RevokeCiTrust,
    RevokeCliSession,
}

const KINDS: [Kind; 14] = [
    Kind::ListAudit,
    Kind::ListMembers,
    Kind::ListInvites,
    Kind::CreateInvite,
    Kind::RevokeInvite,
    Kind::UpdateMemberRole,
    Kind::RemoveMember,
    Kind::ListApiTokens,
    Kind::RevokeApiToken,
    Kind::ListCiTrusts,
    Kind::CreateCiTrust,
    Kind::RevokeCiTrust,
    Kind::ListCliSessions,
    Kind::RevokeCliSession,
];

#[allow(
    clippy::redundant_pub_crate,
    reason = "the catalog module consumes this iterator while this module stays crate-internal"
)]
pub(crate) fn tools() -> impl Iterator<Item = Value> {
    KINDS.into_iter().map(tool)
}

fn tool(kind: Kind) -> Value {
    let name = kind.name();
    let mut properties = scope_properties();
    let (description, mut required) = contract(kind, &mut properties);
    if is_destructive(kind) {
        add(
            &mut properties,
            "confirm",
            boolean("Must be true to confirm this destructive operation."),
        );
        required.push("confirm");
    }
    tool_schema(
        name,
        description,
        &properties,
        &required,
        &annotations(kind),
    )
}

fn contract(kind: Kind, properties: &mut Map<String, Value>) -> (&'static str, Vec<&'static str>) {
    match kind {
        Kind::ListAudit => {
            add(
                properties,
                "cursor",
                string("Optional audit pagination cursor."),
            );
            ("List redacted workspace audit events.", vec!["workspace"])
        }
        Kind::ListMembers => ("List workspace members and seat state.", vec!["workspace"]),
        Kind::ListInvites => ("List redacted workspace invitations.", vec!["workspace"]),
        Kind::ListApiTokens => ("List redacted API token metadata.", vec![]),
        Kind::ListCiTrusts => ("List redacted GitHub OIDC trusts.", vec!["workspace"]),
        Kind::ListCliSessions => ("List active browser-approved CLI sessions.", vec![]),
        Kind::CreateInvite => write_contract(WriteKind::CreateInvite, properties),
        Kind::RevokeInvite => write_contract(WriteKind::RevokeInvite, properties),
        Kind::UpdateMemberRole => write_contract(WriteKind::UpdateMemberRole, properties),
        Kind::RemoveMember => write_contract(WriteKind::RemoveMember, properties),
        Kind::RevokeApiToken => write_contract(WriteKind::RevokeApiToken, properties),
        Kind::CreateCiTrust => write_contract(WriteKind::CreateCiTrust, properties),
        Kind::RevokeCiTrust => write_contract(WriteKind::RevokeCiTrust, properties),
        Kind::RevokeCliSession => write_contract(WriteKind::RevokeCliSession, properties),
    }
}

fn write_contract(
    kind: WriteKind,
    properties: &mut Map<String, Value>,
) -> (&'static str, Vec<&'static str>) {
    match kind {
        WriteKind::CreateInvite => {
            add(
                properties,
                "email",
                string("Invite recipient email address."),
            );
            add(
                properties,
                "role",
                choice("Workspace role.", &["admin", "member", "owner"]),
            );
            (
                "Invite a workspace member.",
                vec!["workspace", "email", "role"],
            )
        }
        WriteKind::RevokeInvite => {
            id_contract(properties, "invite_id", "Revoke a workspace invitation.")
        }
        WriteKind::UpdateMemberRole => {
            add(
                properties,
                "user_id",
                string("Target member user identifier."),
            );
            add(
                properties,
                "role",
                choice("New workspace role.", &["admin", "member", "owner"]),
            );
            (
                "Change a workspace member role.",
                vec!["workspace", "user_id", "role"],
            )
        }
        WriteKind::RemoveMember => {
            add(
                properties,
                "user_id",
                string("Target member user identifier."),
            );
            ("Remove a workspace member.", vec!["workspace", "user_id"])
        }
        WriteKind::RevokeApiToken => id_contract(properties, "token_id", "Revoke an API token."),
        WriteKind::CreateCiTrust => create_trust_contract(properties),
        WriteKind::RevokeCiTrust => {
            id_contract(properties, "trust_id", "Revoke a GitHub OIDC trust.")
        }
        WriteKind::RevokeCliSession => {
            id_contract(properties, "session_id", "Revoke a CLI session.")
        }
    }
}

fn id_contract(
    properties: &mut Map<String, Value>,
    field: &'static str,
    description: &'static str,
) -> (&'static str, Vec<&'static str>) {
    add(properties, field, string("Stable resource identifier."));
    let required = if matches!(field, "invite_id") {
        vec!["workspace", field]
    } else {
        vec![field]
    };
    (description, required)
}

fn create_trust_contract(properties: &mut Map<String, Value>) -> (&'static str, Vec<&'static str>) {
    add(properties, "repository", string("GitHub owner/repository."));
    add(
        properties,
        "workflow_path",
        string("Workflow path under .github/workflows."),
    );
    add(
        properties,
        "workflow_ref",
        string("Pinned workflow git ref."),
    );
    add(
        properties,
        "allowed_ref_glob",
        string("Allowed Git ref glob."),
    );
    add(
        properties,
        "allowed_actions",
        strings("Allowed upload, download, or share actions."),
    );
    add(
        properties,
        "environment",
        string("Optional GitHub environment."),
    );
    (
        "Create a scoped GitHub Actions OIDC trust.",
        vec![
            "workspace",
            "repository",
            "workflow_path",
            "workflow_ref",
            "allowed_ref_glob",
            "allowed_actions",
        ],
    )
}

fn choice(description: &str, values: &[&str]) -> Value {
    json!({ "type": "string", "enum": values, "description": description })
}

fn strings(description: &str) -> Value {
    json!({
        "type": "array", "minItems": 1, "uniqueItems": true,
        "items": { "type": "string", "minLength": 1 }, "description": description
    })
}

fn annotations(kind: Kind) -> Value {
    let read_only = matches!(
        kind,
        Kind::ListAudit
            | Kind::ListMembers
            | Kind::ListInvites
            | Kind::ListApiTokens
            | Kind::ListCiTrusts
            | Kind::ListCliSessions
    );
    let destructive = is_destructive(kind);
    json!({
        "title": title(kind.name()),
        "readOnlyHint": read_only,
        "destructiveHint": destructive,
        "idempotentHint": read_only || destructive,
        "openWorldHint": matches!(kind, Kind::CreateInvite)
    })
}

const fn is_destructive(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::RevokeInvite
            | Kind::UpdateMemberRole
            | Kind::RemoveMember
            | Kind::RevokeApiToken
            | Kind::RevokeCiTrust
            | Kind::RevokeCliSession
    )
}

impl Kind {
    const fn name(self) -> &'static str {
        match self {
            Self::ListAudit => "list_audit",
            Self::ListMembers => "list_members",
            Self::ListInvites => "list_invites",
            Self::CreateInvite => "create_invite",
            Self::RevokeInvite => "revoke_invite",
            Self::UpdateMemberRole => "update_member_role",
            Self::RemoveMember => "remove_member",
            Self::ListApiTokens => "list_api_tokens",
            Self::RevokeApiToken => "revoke_api_token",
            Self::ListCiTrusts => "list_ci_trusts",
            Self::CreateCiTrust => "create_ci_trust",
            Self::RevokeCiTrust => "revoke_ci_trust",
            Self::ListCliSessions => "list_cli_sessions",
            Self::RevokeCliSession => "revoke_cli_session",
        }
    }
}
