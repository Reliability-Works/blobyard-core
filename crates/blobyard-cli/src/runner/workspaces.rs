use super::{Runner, command_result, validate_resource_name};
use crate::headless_commands::{CreateWorkspaceArgs, RenameWorkspaceArgs, WorkspacesCommand};
use blobyard_api_client::{
    ApiRequest, CreateWorkspaceRequest, Endpoint, WorkspacePage, WorkspaceSummary,
};
use blobyard_core::{BlobyardError, ErrorCode};

impl Runner {
    pub(super) async fn execute_workspaces(
        &self,
        command: &WorkspacesCommand,
    ) -> Result<crate::CommandResult, BlobyardError> {
        match command {
            WorkspacesCommand::List => self.list_workspaces().await,
            WorkspacesCommand::Create(arguments) => self.create_workspace(arguments).await,
            WorkspacesCommand::Rename(arguments) => self.rename_workspace(arguments).await,
        }
    }

    pub(super) async fn list_workspaces(&self) -> Result<crate::CommandResult, BlobyardError> {
        let request = ApiRequest::new(Endpoint::ListWorkspaces);
        let success = self.execute_authed::<WorkspacePage>(request).await?;
        let human = workspace_lines(success.data());
        command_result(success.data(), human, success.request_id())
    }

    pub(super) async fn create_workspace(
        &self,
        arguments: &CreateWorkspaceArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        validate_resource_name(&arguments.name, "Workspace")?;
        let request = self.mutation(Endpoint::CreateWorkspace).with_json(
            CreateWorkspaceRequest {
                name: arguments.name.clone(),
            }
            .into_json(),
        );
        let success = self.execute_authed::<WorkspaceSummary>(request).await?;
        let human = format!("Created workspace {}.", success.data().slug());
        command_result(success.data(), human, success.request_id())
    }

    async fn rename_workspace(
        &self,
        arguments: &RenameWorkspaceArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        validate_resource_name(&arguments.name, "Workspace")?;
        let workspace = self.config.workspace().ok_or_else(|| {
            BlobyardError::new(
                ErrorCode::InvalidRequest,
                "Select a workspace with --workspace or Blobyard configuration.",
            )
        })?;
        let request = self
            .mutation(Endpoint::RenameWorkspace)
            .with_json(serde_json::json!({
                "name": arguments.name,
                "workspace": workspace,
            }));
        let success = self.execute_authed::<WorkspaceSummary>(request).await?;
        let human = format!("Renamed workspace {}.", success.data().slug());
        command_result(success.data(), human, success.request_id())
    }
}

fn workspace_lines(page: &WorkspacePage) -> String {
    if page.items().is_empty() {
        return "No workspaces found.".to_owned();
    }
    page.items()
        .iter()
        .map(|workspace| format!("{}\t{}", workspace.slug(), workspace.name()))
        .collect::<Vec<_>>()
        .join("\n")
}
