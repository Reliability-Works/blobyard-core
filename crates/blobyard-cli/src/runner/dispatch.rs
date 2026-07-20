use super::Runner;
use crate::commands::{Command, InboxCommand, ProjectsCommand, RetentionCommand, YardCommand};
use crate::{CommandResult, generate_completion};
use blobyard_core::{BlobyardError, ErrorCode};

impl Runner {
    pub(super) async fn execute_scoped_resource(
        &self,
        command: &Command,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            Command::Projects { command } => self.execute_projects(command).await,
            Command::Inbox { command } => self.execute_inbox(command).await,
            Command::Retention { command } => self.execute_retention(command).await,
            _ => Err(BlobyardError::from_code(ErrorCode::InternalError)),
        }
    }

    pub(super) fn execute_local(&self, command: &Command) -> Result<CommandResult, BlobyardError> {
        match command {
            Command::Init => self.init_project(),
            Command::Completion(arguments) => Ok(CommandResult::local(
                serde_json::json!({ "shell": arguments.shell.to_string() }),
                generate_completion(arguments.shell),
            )),
            _ => Err(BlobyardError::from_code(ErrorCode::InternalError)),
        }
    }

    pub(super) async fn execute_headless(
        &self,
        command: &Command,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            Command::Workspaces { command } => self.execute_workspaces(command).await,
            Command::Shares { command } => self.execute_shares(command).await,
            Command::Previews { command } => self.execute_previews(command).await,
            Command::Billing { .. } | Command::Account { .. } => {
                self.execute_dashboard_command(command).await
            }
            Command::Audit { .. }
            | Command::Members { .. }
            | Command::Invites { .. }
            | Command::Tokens { .. }
            | Command::Trusts { .. }
            | Command::Sessions { .. } => self.execute_admin_command(command).await,
            _ => Err(BlobyardError::from_code(ErrorCode::InternalError)),
        }
    }

    pub(super) async fn execute_yard(
        &self,
        command: &YardCommand,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            YardCommand::List => self.list_yards().await,
            YardCommand::Show(arguments) => self.show_yard(arguments).await,
            YardCommand::History(arguments) => self.yard_history(arguments).await,
            YardCommand::Rollback(arguments) => self.rollback_yard(arguments).await,
            YardCommand::Delete(arguments) => self.delete_yard(arguments).await,
        }
    }

    pub(super) async fn execute_session(
        &self,
        command: &Command,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            Command::Login(arguments) => self.login(arguments).await,
            Command::Logout => self.logout().await,
            Command::Whoami => self.whoami().await,
            _ => Err(BlobyardError::from_code(ErrorCode::InternalError)),
        }
    }

    pub(super) async fn execute_transfer(
        &self,
        command: &Command,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            Command::Upload(arguments) => self.upload(arguments).await,
            Command::Download(arguments) => self.download(arguments).await,
            _ => Err(BlobyardError::from_code(ErrorCode::InternalError)),
        }
    }

    pub(super) async fn execute_projects(
        &self,
        command: &ProjectsCommand,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            ProjectsCommand::List => self.list_projects().await,
            ProjectsCommand::Create(arguments) => self.create_project(arguments).await,
        }
    }

    pub(super) async fn execute_inbox(
        &self,
        command: &InboxCommand,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            InboxCommand::Create(arguments) => self.create_inbox(arguments).await,
            InboxCommand::List => self.list_inboxes().await,
            InboxCommand::Revoke(arguments) => self.revoke_inbox(arguments).await,
        }
    }

    pub(super) async fn execute_retention(
        &self,
        command: &RetentionCommand,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            RetentionCommand::Set(arguments) => self.set_retention(arguments).await,
            RetentionCommand::Show => self.show_retention().await,
            RetentionCommand::Overview => self.retention_overview().await,
            RetentionCommand::Clear => self.clear_retention().await,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::*;
    use crate::runner::login::tests::support::Fixture;

    #[tokio::test]
    async fn grouped_dispatchers_fail_closed_for_unrelated_commands() {
        let fixture = Fixture::new(&["blobyard", "whoami"], vec![]);
        assert_eq!(
            fixture
                .runner
                .execute_scoped_resource(&Command::Whoami)
                .await
                .expect_err("unrelated scoped resource")
                .code(),
            ErrorCode::InternalError
        );
        assert_eq!(
            fixture
                .runner
                .execute_headless(&Command::Whoami)
                .await
                .expect_err("unrelated headless command")
                .code(),
            ErrorCode::InternalError
        );
    }
}
