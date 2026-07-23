use crate::Command;
use crate::yard_commands::{
    AccessCommand, AccessListArgs, DeleteYardArgs, DeployArgs, EnvCommand, EnvListArgs,
    GrantAccessArgs, RevokeAccessArgs, RollbackYardArgs, SetVisibilityArgs, YardCommand,
    YardNameArgs,
};
use blobyard_core::{BlobyardError, ErrorCode};
use blobyard_mcp::{Scope, ToolCall, WebYardToolCall};
use std::path::PathBuf;

pub(super) fn mcp_yard_command(call: ToolCall) -> Result<(Scope, Command), BlobyardError> {
    let ToolCall::WebYard(call) = call else {
        return Err(BlobyardError::from_code(ErrorCode::InternalError));
    };
    let mapped = match call {
        WebYardToolCall::DeployWebYard {
            scope,
            directory,
            yard,
            spa,
            clean_urls,
        } => (scope, deploy_command(directory, yard, spa, clean_urls)),
        WebYardToolCall::ListWebYards { scope } => (scope, yard_command(YardCommand::List)),
        WebYardToolCall::ListYardDeploys { scope, yard } => (
            scope,
            yard_command(YardCommand::History(YardNameArgs { name: yard })),
        ),
        WebYardToolCall::ListYardEnvironments { scope, yard } => (scope, env_command(yard)),
        WebYardToolCall::GetYardAccess { scope, yard } => (scope, access_list_command(yard)),
        WebYardToolCall::SetYardVisibility {
            scope,
            yard,
            visibility,
        } => (scope, visibility_command(yard, visibility)),
        WebYardToolCall::GrantYardAccess {
            scope,
            yard,
            principal_kind,
            principal_id,
            roles,
            environment_id,
            expires_at,
        } => (
            scope,
            grant_command(GrantAccessArgs {
                name: yard,
                principal_kind,
                principal_id,
                roles,
                environment: environment_id,
                expires: expires_at,
            }),
        ),
        WebYardToolCall::RevokeYardAccess {
            scope,
            yard,
            grant_id,
        } => (scope, revoke_command(yard, grant_id)),
        WebYardToolCall::RollbackWebYard {
            scope,
            yard,
            deploy_id,
        } => (scope, rollback_command(yard, deploy_id)),
        WebYardToolCall::DeleteWebYard { scope, yard } => (scope, delete_command(yard)),
    };
    Ok(mapped)
}

const fn yard_command(command: YardCommand) -> Command {
    Command::Yard { command }
}

const fn env_command(yard: String) -> Command {
    Command::Env {
        command: EnvCommand::List(EnvListArgs { name: Some(yard) }),
    }
}

const fn access_list_command(yard: String) -> Command {
    Command::Access {
        command: AccessCommand::List(AccessListArgs { name: Some(yard) }),
    }
}

const fn visibility_command(yard: String, visibility: String) -> Command {
    Command::Access {
        command: AccessCommand::SetVisibility(SetVisibilityArgs {
            name: yard,
            visibility,
        }),
    }
}

const fn grant_command(arguments: GrantAccessArgs) -> Command {
    Command::Access {
        command: AccessCommand::Grant(arguments),
    }
}

const fn revoke_command(yard: String, grant_id: String) -> Command {
    Command::Access {
        command: AccessCommand::Revoke(RevokeAccessArgs {
            name: yard,
            grant_id,
        }),
    }
}

const fn rollback_command(yard: String, deploy_id: Option<String>) -> Command {
    Command::Yard {
        command: YardCommand::Rollback(RollbackYardArgs {
            name: yard,
            deploy_id,
        }),
    }
}

const fn delete_command(yard: String) -> Command {
    Command::Yard {
        command: YardCommand::Delete(DeleteYardArgs {
            name: yard,
            force: true,
        }),
    }
}

fn deploy_command(directory: String, yard: String, spa: bool, clean_urls: bool) -> Command {
    Command::Deploy(DeployArgs {
        directory: Some(PathBuf::from(directory)),
        yard: Some(yard),
        all: false,
        spa,
        clean_urls,
        public: true,
    })
}

#[cfg(test)]
#[path = "mcp_yards_tests.rs"]
mod tests;
