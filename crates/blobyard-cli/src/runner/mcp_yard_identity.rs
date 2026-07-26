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
            | WebYardToolCall::GetYardApplicationPolicy { .. }
            | WebYardToolCall::SetYardApplicationPolicy { .. }
            | WebYardToolCall::SetYardAccessRoles { .. }
    )
}

pub(super) fn command(call: WebYardToolCall) -> Result<(Scope, Command), BlobyardError> {
    match call {
        call @ (WebYardToolCall::ListYardManagementRoles { .. }
        | WebYardToolCall::SetYardManagementRole { .. }
        | WebYardToolCall::RevokeYardManagementRole { .. }) => management_command(call),
        call @ (WebYardToolCall::GetYardApplicationPolicy { .. }
        | WebYardToolCall::SetYardApplicationPolicy { .. }
        | WebYardToolCall::SetYardAccessRoles { .. }) => application_command(call),
        _ => Err(BlobyardError::from_code(ErrorCode::InternalError)),
    }
}

fn management_command(call: WebYardToolCall) -> Result<(Scope, Command), BlobyardError> {
    let mapped = match call {
        WebYardToolCall::ListYardManagementRoles {
            scope,
            yard,
            cursor,
        } => (
            scope,
            Command::ManagementRoles {
                command: ManagementRolesCommand::List(ManagementRolesListArgs {
                    name: yard,
                    cursor,
                }),
            },
        ),
        WebYardToolCall::SetYardManagementRole {
            scope,
            yard,
            user_id,
            role,
        } => (
            scope,
            Command::ManagementRoles {
                command: ManagementRolesCommand::Set(SetManagementRoleArgs {
                    name: yard,
                    user_id,
                    role,
                }),
            },
        ),
        WebYardToolCall::RevokeYardManagementRole {
            scope,
            yard,
            user_id,
        } => (
            scope,
            Command::ManagementRoles {
                command: ManagementRolesCommand::Revoke(RevokeManagementRoleArgs {
                    name: yard,
                    user_id,
                }),
            },
        ),
        _ => return Err(BlobyardError::from_code(ErrorCode::InternalError)),
    };
    Ok(mapped)
}

fn application_command(call: WebYardToolCall) -> Result<(Scope, Command), BlobyardError> {
    let mapped = match call {
        WebYardToolCall::GetYardApplicationPolicy { scope, yard } => (
            scope,
            Command::ApplicationPolicy {
                command: ApplicationPolicyCommand::Get(ApplicationPolicyGetArgs { name: yard }),
            },
        ),
        WebYardToolCall::SetYardApplicationPolicy {
            scope,
            yard,
            source_manifest_digest,
            default_role,
            roles,
        } => (
            scope,
            Command::ApplicationPolicy {
                command: ApplicationPolicyCommand::Set(ApplicationPolicySetArgs {
                    name: yard,
                    policy: None,
                    policy_json: Some(
                        serde_json::json!({
                            "defaultRole": default_role,
                            "roles": roles,
                        })
                        .to_string(),
                    ),
                    source_manifest_digest,
                }),
            },
        ),
        WebYardToolCall::SetYardAccessRoles {
            scope,
            yard,
            grant_id,
            roles,
        } => (
            scope,
            Command::Access {
                command: AccessCommand::SetRoles(SetAccessRolesArgs {
                    name: yard,
                    grant_id,
                    roles,
                }),
            },
        ),
        _ => return Err(BlobyardError::from_code(ErrorCode::InternalError)),
    };
    Ok(mapped)
}
