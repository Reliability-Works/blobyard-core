use crate::{
    Command,
    yard_commands::{
        AccessCommand, ApplicationPolicyCommand, ApplicationPolicyGetArgs,
        ApplicationPolicySetArgs, ManagementRolesCommand, ManagementRolesListArgs,
        RevokeManagementRoleArgs, SetAccessRolesArgs, SetManagementRoleArgs,
    },
};
use blobyard_core::{BlobyardError, ErrorCode};
use blobyard_mcp::{Scope, WebYardToolCall};

pub(super) const fn is_tool(call: &WebYardToolCall) -> bool {
    matches!(
        call,
        WebYardToolCall::ListYardManagementRoles { .. }
            | WebYardToolCall::SetYardManagementRole { .. }
            | WebYardToolCall::RevokeYardManagementRole { .. }
    ) || matches!(
        call,
        WebYardToolCall::GetYardApplicationPolicy { .. }
            | WebYardToolCall::SetYardApplicationPolicy { .. }
            | WebYardToolCall::SetYardAccessRoles { .. }
    )
}

pub(super) fn command(call: WebYardToolCall) -> Result<(Scope, Command), BlobyardError> {
    let mapped = match call {
        WebYardToolCall::ListYardManagementRoles {
            scope,
            yard,
            cursor,
        } => list_roles(scope, yard, cursor),
        WebYardToolCall::SetYardManagementRole {
            scope,
            yard,
            user_id,
            role,
        } => set_role(scope, yard, user_id, role),
        WebYardToolCall::RevokeYardManagementRole {
            scope,
            yard,
            user_id,
        } => revoke_role(scope, yard, user_id),
        WebYardToolCall::GetYardApplicationPolicy { scope, yard } => get_policy(scope, yard),
        WebYardToolCall::SetYardApplicationPolicy {
            scope,
            yard,
            source_manifest_digest,
            default_role,
            roles,
        } => set_policy(
            scope,
            yard,
            source_manifest_digest,
            default_role.as_deref(),
            &roles,
        ),
        WebYardToolCall::SetYardAccessRoles {
            scope,
            yard,
            grant_id,
            roles,
        } => set_access_roles(scope, yard, grant_id, roles),
        _ => return Err(BlobyardError::from_code(ErrorCode::InternalError)),
    };
    Ok(mapped)
}

const fn list_roles(scope: Scope, yard: String, cursor: Option<String>) -> (Scope, Command) {
    (
        scope,
        Command::ManagementRoles {
            command: ManagementRolesCommand::List(ManagementRolesListArgs { name: yard, cursor }),
        },
    )
}

const fn set_role(scope: Scope, yard: String, user_id: String, role: String) -> (Scope, Command) {
    (
        scope,
        Command::ManagementRoles {
            command: ManagementRolesCommand::Set(SetManagementRoleArgs {
                name: yard,
                user_id,
                role,
            }),
        },
    )
}

const fn revoke_role(scope: Scope, yard: String, user_id: String) -> (Scope, Command) {
    (
        scope,
        Command::ManagementRoles {
            command: ManagementRolesCommand::Revoke(RevokeManagementRoleArgs {
                name: yard,
                user_id,
            }),
        },
    )
}

const fn get_policy(scope: Scope, yard: String) -> (Scope, Command) {
    (
        scope,
        Command::ApplicationPolicy {
            command: ApplicationPolicyCommand::Get(ApplicationPolicyGetArgs { name: yard }),
        },
    )
}

fn set_policy(
    scope: Scope,
    yard: String,
    source_manifest_digest: String,
    default_role: Option<&str>,
    roles: &serde_json::Value,
) -> (Scope, Command) {
    let policy_json = serde_json::json!({
        "defaultRole": default_role,
        "roles": roles,
    })
    .to_string();
    (
        scope,
        Command::ApplicationPolicy {
            command: ApplicationPolicyCommand::Set(ApplicationPolicySetArgs {
                name: yard,
                policy: None,
                policy_json: Some(policy_json),
                source_manifest_digest,
            }),
        },
    )
}

const fn set_access_roles(
    scope: Scope,
    yard: String,
    grant_id: String,
    roles: Vec<String>,
) -> (Scope, Command) {
    (
        scope,
        Command::Access {
            command: AccessCommand::SetRoles(SetAccessRolesArgs {
                name: yard,
                grant_id,
                roles,
            }),
        },
    )
}
