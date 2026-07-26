use super::{Runner, command_result};
use crate::headless_commands::GroupsCommand;
use blobyard_contract::normalize_group_name;
use blobyard_core::{BlobyardError, ErrorCode};
use blobyard_mcp::{AdminToolCall, GroupToolCall, Scope};

impl Runner {
    pub(super) async fn execute_groups_command(
        &self,
        command: &GroupsCommand,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let call = group_call(command)?;
        let success = self
            .execute_mcp_admin_success(AdminToolCall::Group(call))
            .await?;
        let human = group_human(command, success.data());
        command_result(success.data(), human, success.request_id())
    }
}

fn group_call(command: &GroupsCommand) -> Result<GroupToolCall, BlobyardError> {
    let scope = Scope::default();
    let call = match command {
        GroupsCommand::List(arguments) => GroupToolCall::List {
            scope,
            cursor: arguments.cursor.clone(),
        },
        GroupsCommand::Create(arguments) => GroupToolCall::Create {
            scope,
            name: normalized_name(&arguments.name)?,
        },
        GroupsCommand::Rename(arguments) => GroupToolCall::Rename {
            scope,
            group_id: arguments.group_id.clone(),
            name: normalized_name(&arguments.name)?,
        },
        GroupsCommand::Members(arguments) => GroupToolCall::ListMembers {
            scope,
            group_id: arguments.group_id.clone(),
            cursor: arguments.cursor.clone(),
        },
        GroupsCommand::AddMember(arguments) => GroupToolCall::AddMember {
            scope,
            group_id: arguments.group_id.clone(),
            user_id: arguments.user_id.clone(),
        },
        GroupsCommand::RemoveMember(arguments) => GroupToolCall::RemoveMember {
            scope,
            group_id: arguments.group_id.clone(),
            user_id: arguments.user_id.clone(),
            confirmed: true,
        },
        GroupsCommand::Deactivate(arguments) => GroupToolCall::Deactivate {
            scope,
            group_id: arguments.group_id.clone(),
            confirmed: true,
        },
    };
    Ok(call)
}

fn normalized_name(value: &str) -> Result<String, BlobyardError> {
    normalize_group_name(value).map_err(|_error| {
        BlobyardError::new(
            ErrorCode::InvalidRequest,
            "Group names must contain 2-80 printable characters.",
        )
    })
}

fn group_human(command: &GroupsCommand, value: &serde_json::Value) -> String {
    match command {
        GroupsCommand::Create(_) => "Group created.".to_owned(),
        GroupsCommand::Rename(_) => "Group renamed.".to_owned(),
        GroupsCommand::AddMember(_) => "Group member added.".to_owned(),
        GroupsCommand::RemoveMember(_) => "Group member removed.".to_owned(),
        GroupsCommand::Deactivate(_) => "Group deactivated.".to_owned(),
        GroupsCommand::List(_) | GroupsCommand::Members(_) => format!("{value:#}"),
    }
}

#[cfg(test)]
#[path = "groups_edge_tests.rs"]
mod edge_tests;

#[cfg(test)]
#[path = "groups_tests.rs"]
mod tests;
