use super::Runner;
use crate::commands::{Command, InboxCommand, ProjectsCommand, RetentionCommand};
use crate::yard_commands::{
    AccessCommand, ApplicationPolicyCommand, EnvCommand, GuestInvitesCommand,
    ManagementRolesCommand, YardCommand, YardSessionsCommand,
};
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
            Command::App { command } => {
                crate::app_manifest::execute(command, self.config.paths().cwd())
            }
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
            Command::Users { command } => self.execute_users_command(command).await,
            Command::Groups { command } => self.execute_groups_command(command).await,
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

    pub(super) async fn execute_env(
        &self,
        command: &EnvCommand,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            EnvCommand::List(arguments) => self.list_environments(arguments).await,
        }
    }

    pub(super) async fn execute_yard_family(
        &self,
        command: &Command,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            Command::Yard { command } => self.execute_yard(command).await,
            Command::Env { command } => self.execute_env(command).await,
            Command::Access { command } => self.execute_access(command).await,
            Command::GuestInvites { command } => self.execute_guest_invites(command).await,
            Command::ManagementRoles { command } => self.execute_management_roles(command).await,
            Command::ApplicationPolicy { command } => {
                self.execute_application_policy(command).await
            }
            Command::YardSessions { command } => self.execute_yard_sessions(command).await,
            _ => Err(BlobyardError::from_code(ErrorCode::InternalError)),
        }
    }

    pub(super) async fn execute_guest_invites(
        &self,
        command: &GuestInvitesCommand,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            GuestInvitesCommand::List(arguments) => self.list_guest_invites(arguments).await,
            GuestInvitesCommand::Create(arguments) => self.create_guest_invite(arguments).await,
            GuestInvitesCommand::Revoke(arguments) => self.revoke_guest_invite(arguments).await,
        }
    }

    pub(super) async fn execute_access(
        &self,
        command: &AccessCommand,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            AccessCommand::List(arguments) => self.access_list(arguments).await,
            AccessCommand::SetVisibility(arguments) => self.access_set_visibility(arguments).await,
            AccessCommand::Grant(arguments) => self.access_grant(arguments).await,
            AccessCommand::Revoke(arguments) => self.access_revoke(arguments).await,
            AccessCommand::SetRoles(arguments) => self.access_set_roles(arguments).await,
        }
    }

    pub(super) async fn execute_management_roles(
        &self,
        command: &ManagementRolesCommand,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            ManagementRolesCommand::List(arguments) => self.list_management_roles(arguments).await,
            ManagementRolesCommand::Set(arguments) => self.set_management_role(arguments).await,
            ManagementRolesCommand::Revoke(arguments) => {
                self.revoke_management_role(arguments).await
            }
        }
    }

    pub(super) async fn execute_application_policy(
        &self,
        command: &ApplicationPolicyCommand,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            ApplicationPolicyCommand::Get(arguments) => {
                self.get_application_policy(arguments).await
            }
            ApplicationPolicyCommand::Set(arguments) => {
                self.set_application_policy(arguments).await
            }
        }
    }

    pub(super) async fn execute_yard_sessions(
        &self,
        command: &YardSessionsCommand,
    ) -> Result<CommandResult, BlobyardError> {
        match command {
            YardSessionsCommand::List(arguments) => self.list_yard_sessions(arguments).await,
            YardSessionsCommand::Revoke(arguments) => self.revoke_yard_session(arguments).await,
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
        assert_eq!(
            fixture
                .runner
                .execute_yard_family(&Command::Whoami)
                .await
                .expect_err("unrelated Yard-family command")
                .code(),
            ErrorCode::InternalError
        );
    }
}
