use super::sharing::{validate_duration, validate_identifier};
use super::{Runner, command_result};
use crate::commands::{Command, PreviewArgs, UploadArgs};
use crate::headless_commands::{PreviewsCommand, RevokePreviewArgs};
use blobyard_api_client::{
    ApiRequest, CreatePreviewRequest, CreatePreviewResponse, EmptyResponse, Endpoint,
    ListPreviewsQuery, PreviewPage, RevokePreviewRequest,
};
use blobyard_core::{BlobyardError, ErrorCode};
use std::path::Path;

const PREVIEW_MANIFEST_ROOT: &str = ".blobyard-preview";

impl Runner {
    pub(in crate::runner) async fn execute_previews(
        &self,
        command: &PreviewsCommand,
    ) -> Result<crate::CommandResult, BlobyardError> {
        match command {
            PreviewsCommand::List => self.list_previews().await,
            PreviewsCommand::Revoke(arguments) => self.revoke_preview(arguments).await,
        }
    }

    pub(in crate::runner) async fn execute_capability(
        &self,
        command: &Command,
    ) -> Result<crate::CommandResult, BlobyardError> {
        match command {
            Command::Share(arguments) => self.create_share(arguments).await,
            Command::Preview(arguments) => self.create_preview(arguments).await,
            _ => Err(BlobyardError::from_code(ErrorCode::InternalError)),
        }
    }

    pub(super) async fn create_preview(
        &self,
        arguments: &PreviewArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        validate_duration(arguments.expires.as_deref())?;
        let (workspace, project) = self.scope()?;
        require_index(&arguments.directory)?;
        let manifest_id = uuid::Uuid::new_v4().simple().to_string();
        let uploads = UploadArgs {
            source: arguments.directory.clone(),
            path: Some(format!("{PREVIEW_MANIFEST_ROOT}/{manifest_id}")),
            include_ignored: false,
        };
        let (_objects, _upload_request_id) = self.upload_files(&uploads).await?;
        let request = self.mutation(Endpoint::CreatePreview).with_json(
            CreatePreviewRequest {
                workspace,
                project,
                manifest_id,
                expires: arguments.expires.clone(),
            }
            .into_json(),
        );
        let success = self
            .execute_authed::<CreatePreviewResponse>(request)
            .await?;
        let human = success.data().preview_url.expose_secret().to_owned();
        command_result(success.data(), human, success.request_id())
    }

    pub(super) async fn list_previews(&self) -> Result<crate::CommandResult, BlobyardError> {
        let (workspace, project) = self.scope()?;
        let request = ApiRequest::new(Endpoint::ListPreviews)
            .with_query(ListPreviewsQuery { workspace, project }.into_query());
        let success = self.execute_authed::<PreviewPage>(request).await?;
        let human = if success.data().items().is_empty() {
            "No previews found.".to_owned()
        } else {
            success
                .data()
                .items()
                .iter()
                .map(|preview| {
                    format!("{}\t{}\t{}", preview.id, preview.status, preview.expires_at)
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        command_result(success.data(), human, success.request_id())
    }

    pub(super) async fn revoke_preview(
        &self,
        arguments: &RevokePreviewArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        validate_identifier(&arguments.preview_id, "preview")?;
        let request = self.mutation(Endpoint::RevokePreview).with_json(
            RevokePreviewRequest {
                preview_id: arguments.preview_id.clone(),
            }
            .into_json(),
        );
        let success = self.execute_authed::<EmptyResponse>(request).await?;
        command_result(success.data(), "Preview revoked.", success.request_id())
    }
}

pub(super) fn require_index(directory: &Path) -> Result<(), BlobyardError> {
    let entry = directory.join("index.html");
    let valid =
        std::fs::symlink_metadata(entry).is_ok_and(|metadata| metadata.file_type().is_file());
    if valid {
        Ok(())
    } else {
        Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            "The preview directory must contain a regular index.html file.",
        ))
    }
}
