use super::{Runner, command_result};
use crate::commands::{ListArgs, RemoveArgs};
use blobyard_api_client::{
    ApiRequest, DeleteObjectRequest, DeleteObjectResponse, Endpoint, ListObjectsQuery, ObjectPage,
};
use blobyard_core::{BlobyardError, BlobyardUri};
use std::str::FromStr;

impl Runner {
    pub(super) async fn list_objects(
        &self,
        arguments: &ListArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let (workspace, project, prefix) = if let Some(prefix) = &arguments.prefix {
            let uri = BlobyardUri::from_str(prefix).map_err(invalid_uri)?;
            (
                uri.workspace_slug().clone(),
                uri.project_slug().clone(),
                Some(uri.logical_path().to_owned()),
            )
        } else {
            let (workspace, project) = self.scope()?;
            (workspace, project, None)
        };
        let request = ApiRequest::new(Endpoint::ListObjects).with_query(
            ListObjectsQuery {
                workspace,
                project,
                prefix,
                versions: arguments.versions,
                cursor: None,
            }
            .into_query(),
        );
        let success = self.execute_authed::<ObjectPage>(request).await?;
        let human = object_lines(success.data());
        command_result(success.data(), human, success.request_id())
    }

    pub(super) async fn remove_object(
        &self,
        arguments: &RemoveArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let uri = BlobyardUri::from_str(&arguments.uri).map_err(invalid_uri)?;
        let request = self
            .mutation(Endpoint::DeleteObject)
            .with_json(DeleteObjectRequest { uri }.into_json());
        let success = self.execute_authed::<DeleteObjectResponse>(request).await?;
        let human = format!("Removed {}.", success.data().uri);
        command_result(success.data(), human, success.request_id())
    }
}

fn object_lines(page: &ObjectPage) -> String {
    if page.items().is_empty() {
        return "No objects found.".to_owned();
    }
    page.items()
        .iter()
        .map(|object| format!("{}\t{} bytes", object.uri, object.size_bytes))
        .collect::<Vec<_>>()
        .join("\n")
}

fn invalid_uri(_error: blobyard_core::BlobyardUriError) -> BlobyardError {
    BlobyardError::new(
        blobyard_core::ErrorCode::InvalidRequest,
        "The Blobyard URI isn't valid. Check it and try again.",
    )
}
