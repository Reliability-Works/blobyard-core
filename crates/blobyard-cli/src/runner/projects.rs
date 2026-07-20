use super::{Runner, command_result, validate_resource_name};
use crate::commands::CreateProjectArgs;
use blobyard_api_client::{
    ApiRequest, CreateProjectRequest, Endpoint, ListProjectsQuery, ProjectPage, ProjectSummary,
};
use blobyard_core::{BlobyardError, ErrorCode};

impl Runner {
    pub(super) async fn list_projects(&self) -> Result<crate::CommandResult, BlobyardError> {
        let workspace = self.config.workspace().cloned().ok_or_else(|| {
            BlobyardError::new(
                ErrorCode::InvalidRequest,
                "Select a workspace with --workspace or Blobyard configuration.",
            )
        })?;
        let request = ApiRequest::new(Endpoint::ListProjects).with_query(
            ListProjectsQuery {
                workspace,
                cursor: None,
            }
            .into_query(),
        );
        let success = self.execute_authed::<ProjectPage>(request).await?;
        let human = project_lines(success.data());
        command_result(success.data(), human, success.request_id())
    }

    pub(super) async fn create_project(
        &self,
        arguments: &CreateProjectArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        validate_resource_name(&arguments.name, "Project")?;
        let workspace = self.config.workspace().cloned().ok_or_else(|| {
            BlobyardError::new(
                ErrorCode::InvalidRequest,
                "Select a workspace with --workspace or Blobyard configuration.",
            )
        })?;
        let request = self.mutation(Endpoint::CreateProject).with_json(
            CreateProjectRequest {
                workspace,
                name: arguments.name.clone(),
            }
            .into_json(),
        );
        let success = self.execute_authed::<ProjectSummary>(request).await?;
        let human = format!("Created project {}.", success.data().slug());
        command_result(success.data(), human, success.request_id())
    }
}

fn project_lines(page: &ProjectPage) -> String {
    if page.items().is_empty() {
        return "No projects found.".to_owned();
    }
    page.items()
        .iter()
        .map(|project| format!("{}\t{}", project.slug(), project.name()))
        .collect::<Vec<_>>()
        .join("\n")
}
