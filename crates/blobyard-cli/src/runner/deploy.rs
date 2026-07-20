use super::deploy_output::{
    DeployBatchOutput, DeployItem, DeployOutput, batch_detail, deploy_output, detail,
};
use super::deploy_selection::{SelectedYard, select};
use super::preview::require_index;
use super::{Runner, command_result};
use crate::commands::{DeployArgs, UploadArgs};
use crate::{CommandResult, OutputMode};
use blobyard_api_client::{
    ApiRequest, EmptyResponse, Endpoint, FailYardDeployRequest, StartYardDeployRequest,
    StartYardDeployResponse, YardDeployMutationRequest, YardDeployStatus, YardDeploymentResponse,
};
use blobyard_core::{BlobyardError, ErrorCode, Slug};
use std::future::Future;
use std::pin::Pin;

type DeployResult = Result<(YardDeploymentResponse, String), BlobyardError>;
type DeployFuture<'a> = Pin<Box<dyn Future<Output = DeployResult> + Send + 'a>>;
type InterruptionFuture<'a> = Pin<Box<dyn Future<Output = std::io::Result<()>> + Send + 'a>>;

impl Runner {
    pub(super) async fn deploy(
        &self,
        arguments: &DeployArgs,
    ) -> Result<CommandResult, BlobyardError> {
        let selected = select(self.config.yards(), arguments)?;
        if !arguments.all {
            let (output, request_id) = self.deploy_one(&selected[0], arguments.public).await?;
            let human = detail(&output);
            return command_result(&output, human, &request_id);
        }
        self.deploy_all(&selected, arguments.public).await
    }

    async fn deploy_all(
        &self,
        selected: &[SelectedYard],
        public: bool,
    ) -> Result<CommandResult, BlobyardError> {
        let mut results = Vec::with_capacity(selected.len());
        let mut first_error = None;
        for yard in selected {
            match self.deploy_one(yard, public).await {
                Ok((output, _request_id)) => results.push(DeployItem::success(output)),
                Err(error) => {
                    first_error.get_or_insert_with(|| error.clone());
                    results.push(DeployItem::failure(yard.name.to_string(), &error));
                }
            }
        }
        let human = batch_detail(&results);
        let data = serde_json::json!(DeployBatchOutput { results });
        if let Some(error) = first_error {
            Ok(CommandResult::partial_failure(
                data,
                human,
                BlobyardError::new(
                    error.code(),
                    "One or more Web Yard deploys failed. Review each result and retry the failed Yards.",
                ),
            ))
        } else {
            Ok(CommandResult::local(data, human))
        }
    }

    async fn deploy_one(
        &self,
        selected: &SelectedYard,
        public: bool,
    ) -> Result<(DeployOutput, String), BlobyardError> {
        require_index(&selected.directory).map_err(|_error| {
            BlobyardError::new(
                ErrorCode::InvalidRequest,
                "The Web Yard directory must contain a regular index.html file.",
            )
        })?;
        self.ensure_public_consent(&selected.name, public).await?;
        let (workspace, project) = self.scope()?;
        let client_deploy_id = uuid::Uuid::new_v4().simple().to_string();
        let started = self
            .start_deploy(&workspace, &project, selected, &client_deploy_id)
            .await?;
        let result = until_interrupted(
            Box::pin(self.upload_and_finalise(selected, &started)),
            Box::pin(tokio::signal::ctrl_c()),
        )
        .await;
        match result {
            Ok((response, request_id)) => Ok((deploy_output(&started, &response), request_id)),
            Err(error) => {
                let _ignored = self.fail_deploy(&started.deploy_id, &error).await;
                Err(error)
            }
        }
    }

    async fn ensure_public_consent(&self, name: &Slug, public: bool) -> Result<(), BlobyardError> {
        if public {
            return Ok(());
        }
        if self.output_mode != OutputMode::Human || !self.confirmation.is_interactive() {
            return Err(public_flag_required());
        }
        let (yards, _request_id) = self.all_web_yards().await?;
        let already_public = yards
            .iter()
            .find(|yard| &yard.name == name)
            .is_some_and(|yard| yard.current_deploy_id.is_some());
        if already_public {
            return Ok(());
        }
        let origin = self.config.web_yard_origin().as_str();
        let prompt = format!("Deploy Web Yard '{name}' to a public address under {origin}? [y/N] ");
        if self.confirmation.confirm(&prompt)? {
            Ok(())
        } else {
            Err(BlobyardError::from_code(ErrorCode::Interrupted))
        }
    }

    async fn start_deploy(
        &self,
        workspace: &Slug,
        project: &Slug,
        selected: &SelectedYard,
        client_deploy_id: &str,
    ) -> Result<StartYardDeployResponse, BlobyardError> {
        let request = ApiRequest::new(Endpoint::StartYardDeploy).with_json(
            StartYardDeployRequest {
                workspace: workspace.clone(),
                project: project.clone(),
                name: selected.name.clone(),
                client_deploy_id: client_deploy_id.to_owned(),
                spa: selected.spa,
                clean_urls: selected.clean_urls,
                public: true,
            }
            .into_json(),
        );
        let started = self
            .execute_authed::<StartYardDeployResponse>(request)
            .await?
            .into_data();
        if let Err(error) = validate_start(&started, selected, self.config.web_yard_origin()) {
            let _ignored = self.fail_deploy(&started.deploy_id, &error).await;
            return Err(error);
        }
        Ok(started)
    }

    async fn upload_and_finalise(
        &self,
        selected: &SelectedYard,
        started: &StartYardDeployResponse,
    ) -> Result<(YardDeploymentResponse, String), BlobyardError> {
        let manifest_prefix = started.manifest_root.trim_end_matches('/');
        let uploads = UploadArgs {
            source: selected.directory.clone(),
            path: Some(manifest_prefix.to_owned()),
            include_ignored: false,
        };
        let (_objects, _request_id) = self.upload_files(&uploads).await?;
        let request = ApiRequest::new(Endpoint::FinaliseYardDeploy).with_json(
            YardDeployMutationRequest {
                deploy_id: started.deploy_id.clone(),
            }
            .into_json(),
        );
        let success = self
            .execute_authed::<YardDeploymentResponse>(request)
            .await?;
        if success.data().deploy_id != started.deploy_id
            || success.data().url != started.url
            || success.data().deployment_url != started.deployment_url
        {
            return Err(inconsistent_metadata());
        }
        let request_id = success.request_id().to_owned();
        Ok((success.into_data(), request_id))
    }

    async fn fail_deploy(
        &self,
        deploy_id: &str,
        error: &BlobyardError,
    ) -> Result<(), BlobyardError> {
        let request = ApiRequest::new(Endpoint::FailYardDeploy).with_json(
            FailYardDeployRequest {
                deploy_id: deploy_id.to_owned(),
                failure_code: error.code().as_str().to_owned(),
                failure_message: error.message().to_owned(),
            }
            .into_json(),
        );
        self.execute_authed::<EmptyResponse>(request)
            .await
            .map(|_success| ())
    }
}

fn validate_start(
    started: &StartYardDeployResponse,
    selected: &SelectedYard,
    origin: &blobyard_core::WebYardOrigin,
) -> Result<(), BlobyardError> {
    let valid = started.yard_name == selected.name
        && started.status == YardDeployStatus::Uploading
        && !started.deploy_id.is_empty()
        && valid_host_label(&started.host_label)
        && has_lower_hex_segment(&started.host_label, 9)
        && origin.matches(&started.url, &started.host_label)
        && valid_deployment_url(&started.deployment_url, &started.host_label, origin)
        && manifest_prefix(started).is_some();
    if valid {
        Ok(())
    } else {
        Err(inconsistent_metadata())
    }
}

fn valid_deployment_url(
    value: &str,
    stable_host_label: &str,
    origin: &blobyard_core::WebYardOrigin,
) -> bool {
    let Some(host_label) = web_yard_host_label(value, origin) else {
        return false;
    };
    valid_host_label(host_label)
        && stable_host_label.contains('-')
        && has_lower_hex_segment(host_label, 10)
}

fn web_yard_host_label<'a>(
    value: &'a str,
    origin: &blobyard_core::WebYardOrigin,
) -> Option<&'a str> {
    let prefix = value.strip_suffix(&format!(".{}", origin.authority()))?;
    let host_label = prefix.split_once("://")?.1;
    origin.matches(value, host_label).then_some(host_label)
}

fn has_lower_hex_segment(host_label: &str, length: usize) -> bool {
    host_label.split('-').any(|part| {
        part.len() == length
            && part
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn valid_host_label(value: &str) -> bool {
    value.contains('-')
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value.len() <= 63
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn manifest_prefix(started: &StartYardDeployResponse) -> Option<&str> {
    let prefix = started.manifest_root.strip_suffix('/')?;
    let identity = prefix.strip_prefix(".blobyard-yard/")?;
    let (yard_id, client_id) = identity.split_once('/')?;
    let valid = yard_id == started.yard_id
        && !client_id.is_empty()
        && !client_id.contains('/')
        && !client_id.chars().any(char::is_control);
    valid.then_some(prefix)
}

fn inconsistent_metadata() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::ProviderUnavailable,
        "Blobyard returned inconsistent Web Yard deploy metadata. Try again shortly.",
    )
}

fn public_flag_required() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InvalidRequest,
        "Web Yards are public. Re-run with --public to acknowledge that boundary.",
    )
}

async fn until_interrupted(
    work: DeployFuture<'_>,
    interruption: InterruptionFuture<'_>,
) -> DeployResult {
    tokio::select! {
        result = work => result,
        _ = interruption => Err(BlobyardError::from_code(ErrorCode::Interrupted)),
    }
}

#[cfg(test)]
#[path = "deploy_tests.rs"]
mod tests;
