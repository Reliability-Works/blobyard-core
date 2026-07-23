#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::mcp_yard_command;
use crate::Command;
use crate::yard_commands::{EnvCommand, YardCommand};
use blobyard_mcp::{Scope, ToolCall, WebYardToolCall};

#[test]
fn web_yard_tools_map_to_confirmed_cli_contracts() {
    let scope = Scope::default();
    let (_, deploy) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::DeployWebYard {
        scope: scope.clone(),
        directory: "./dist".into(),
        yard: "site".into(),
        spa: true,
        clean_urls: false,
    }))
    .expect("deploy mapping");
    assert!(matches!(deploy, Command::Deploy(arguments) if arguments.public));
    let (_, delete) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::DeleteWebYard {
        scope,
        yard: "site".into(),
    }))
    .expect("delete mapping");
    assert!(matches!(
        delete,
        Command::Yard {
            command: YardCommand::Delete(arguments)
        } if arguments.force
    ));
}

#[test]
fn web_yard_management_tools_map_to_cli_contracts() {
    let scope = Scope::default();
    let (_, list) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::ListWebYards {
        scope: scope.clone(),
    }))
    .expect("list mapping");
    assert!(matches!(
        list,
        Command::Yard {
            command: YardCommand::List
        }
    ));
    let (_, history) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::ListYardDeploys {
        scope: scope.clone(),
        yard: "site".into(),
    }))
    .expect("history mapping");
    assert!(matches!(
        history,
        Command::Yard {
            command: YardCommand::History(_)
        }
    ));
    let (_, environments) =
        mcp_yard_command(ToolCall::WebYard(WebYardToolCall::ListYardEnvironments {
            scope: scope.clone(),
            yard: "site".into(),
        }))
        .expect("environment mapping");
    assert!(matches!(
        environments,
        Command::Env {
            command: EnvCommand::List(_)
        }
    ));
    let (_, rollback) = mcp_yard_command(ToolCall::WebYard(WebYardToolCall::RollbackWebYard {
        scope,
        yard: "site".into(),
        deploy_id: Some("deploy_1".into()),
    }))
    .expect("rollback mapping");
    assert!(matches!(
        rollback,
        Command::Yard {
            command: YardCommand::Rollback(_)
        }
    ));
}
