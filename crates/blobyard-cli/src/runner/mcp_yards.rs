use crate::Command;
use crate::commands::{DeleteYardArgs, DeployArgs, RollbackYardArgs, YardCommand, YardNameArgs};
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
        } => (
            scope,
            Command::Deploy(DeployArgs {
                directory: Some(PathBuf::from(directory)),
                yard: Some(yard),
                all: false,
                spa,
                clean_urls,
                public: true,
            }),
        ),
        WebYardToolCall::ListWebYards { scope } => (
            scope,
            Command::Yard {
                command: YardCommand::List,
            },
        ),
        WebYardToolCall::ListYardDeploys { scope, yard } => (
            scope,
            Command::Yard {
                command: YardCommand::History(YardNameArgs { name: yard }),
            },
        ),
        WebYardToolCall::RollbackWebYard {
            scope,
            yard,
            deploy_id,
        } => (
            scope,
            Command::Yard {
                command: YardCommand::Rollback(RollbackYardArgs {
                    name: yard,
                    deploy_id,
                }),
            },
        ),
        WebYardToolCall::DeleteWebYard { scope, yard } => (
            scope,
            Command::Yard {
                command: YardCommand::Delete(DeleteYardArgs {
                    name: yard,
                    force: true,
                }),
            },
        ),
    };
    Ok(mapped)
}
