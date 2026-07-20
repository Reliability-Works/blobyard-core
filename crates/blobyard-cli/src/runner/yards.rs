use super::{Runner, command_result};
use crate::OutputMode;
use crate::commands::{DeleteYardArgs, RollbackYardArgs, ShowYardArgs, YardNameArgs};
use crate::config::validate_yard_name;
use blobyard_api_client::{
    ApiRequest, DeleteWebYardRequest, EmptyResponse, Endpoint, ListWebYardsQuery,
    ListYardDeploysQuery, RollbackWebYardRequest, WebYardPage, WebYardSummary, YardDeployPage,
    YardDeployStatus, YardDeploySummary, YardDeploymentResponse,
};
use blobyard_core::{BlobyardError, ErrorCode, Slug};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct YardListOutput<'a> {
    yards: &'a [WebYardSummary],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct YardHistoryOutput<'a> {
    yard: &'a Slug,
    deploys: &'a [YardDeploySummary],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct YardMutationOutput<'a> {
    yard: &'a Slug,
    yard_url: &'a str,
    deployment_url: &'a str,
    deploy_id: &'a str,
    status: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct YardDeleteOutput<'a> {
    yard: &'a Slug,
    deleted: bool,
}

impl Runner {
    pub(super) async fn all_web_yards(
        &self,
    ) -> Result<(Vec<WebYardSummary>, String), BlobyardError> {
        let (workspace, project) = self.scope()?;
        let request = ApiRequest::new(Endpoint::ListWebYards)
            .with_query(ListWebYardsQuery { workspace, project }.into_query());
        let success = self.execute_authed::<WebYardPage>(request).await?;
        ensure_unpaginated(success.data())?;
        Ok((
            success.data().items().to_vec(),
            success.request_id().to_owned(),
        ))
    }

    pub(super) async fn list_yards(&self) -> Result<crate::CommandResult, BlobyardError> {
        let (yards, request_id) = self.all_web_yards().await?;
        let human = yard_lines(&yards);
        command_result(&YardListOutput { yards: &yards }, human, &request_id)
    }

    pub(super) async fn show_yard(
        &self,
        arguments: &ShowYardArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let (yards, request_id) = self.all_web_yards().await?;
        let yard = select_yard(&yards, arguments.name.as_deref())?;
        command_result(yard, yard_detail(yard), &request_id)
    }

    pub(super) async fn yard_history(
        &self,
        arguments: &YardNameArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let yard = validate_yard_name(&arguments.name)?;
        let (yards, _request_id) = self.all_web_yards().await?;
        let selected = named_yard(&yards, &yard)?;
        let (deploys, request_id) = self.all_yard_deploys(&selected.id).await?;
        let human = deploy_lines(&deploys);
        command_result(
            &YardHistoryOutput {
                yard: &yard,
                deploys: &deploys,
            },
            human,
            &request_id,
        )
    }

    pub(super) async fn rollback_yard(
        &self,
        arguments: &RollbackYardArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let yard = validate_yard_name(&arguments.name)?;
        validate_deploy_id(arguments.deploy_id.as_deref())?;
        let (yards, _request_id) = self.all_web_yards().await?;
        let selected = named_yard(&yards, &yard)?;
        let request = self.mutation(Endpoint::RollbackWebYard).with_json(
            RollbackWebYardRequest {
                yard_id: selected.id.clone(),
                deploy_id: arguments.deploy_id.clone(),
            }
            .into_json(),
        );
        let success = self
            .execute_authed::<YardDeploymentResponse>(request)
            .await?;
        let output = yard_mutation_output(selected, success.data());
        command_result(&output, deployment_detail(&output), success.request_id())
    }

    pub(super) async fn delete_yard(
        &self,
        arguments: &DeleteYardArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let yard = validate_yard_name(&arguments.name)?;
        self.confirm_delete(&yard, arguments.force)?;
        let (yards, _request_id) = self.all_web_yards().await?;
        let selected = named_yard(&yards, &yard)?;
        let request = self.mutation(Endpoint::DeleteWebYard).with_json(
            DeleteWebYardRequest {
                yard_id: selected.id.clone(),
            }
            .into_json(),
        );
        let success = self.execute_authed::<EmptyResponse>(request).await?;
        let output = YardDeleteOutput {
            yard: &yard,
            deleted: true,
        };
        command_result(
            &output,
            format!("Deleted Web Yard '{yard}'."),
            success.request_id(),
        )
    }

    async fn all_yard_deploys(
        &self,
        yard_id: &str,
    ) -> Result<(Vec<YardDeploySummary>, String), BlobyardError> {
        let request = ApiRequest::new(Endpoint::ListYardDeploys).with_query(
            ListYardDeploysQuery {
                yard_id: yard_id.to_owned(),
            }
            .into_query(),
        );
        let success = self.execute_authed::<YardDeployPage>(request).await?;
        ensure_unpaginated(success.data())?;
        Ok((
            success.data().items().to_vec(),
            success.request_id().to_owned(),
        ))
    }

    fn confirm_delete(&self, yard: &Slug, force: bool) -> Result<(), BlobyardError> {
        if force {
            return Ok(());
        }
        if self.output_mode != OutputMode::Human || !self.confirmation.is_interactive() {
            return Err(BlobyardError::new(
                ErrorCode::InvalidRequest,
                "Deleting a Web Yard is destructive. Re-run with --force to confirm.",
            ));
        }
        let prompt = format!("Delete Web Yard '{yard}' and all of its immutable deploys? [y/N] ");
        if self.confirmation.confirm(&prompt)? {
            Ok(())
        } else {
            Err(BlobyardError::from_code(ErrorCode::Interrupted))
        }
    }
}

fn named_yard<'a>(
    yards: &'a [WebYardSummary],
    name: &Slug,
) -> Result<&'a WebYardSummary, BlobyardError> {
    yards
        .iter()
        .find(|yard| &yard.name == name)
        .ok_or_else(|| BlobyardError::from_code(ErrorCode::NotFound))
}

fn select_yard<'a>(
    yards: &'a [WebYardSummary],
    name: Option<&str>,
) -> Result<&'a WebYardSummary, BlobyardError> {
    if let Some(name) = name {
        let name = validate_yard_name(name)?;
        return named_yard(yards, &name);
    }
    match yards {
        [yard] => Ok(yard),
        [] => Err(BlobyardError::from_code(ErrorCode::NotFound)),
        _ => Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            "Select a Web Yard by name. More than one Web Yard exists in this project.",
        )),
    }
}

fn yard_lines(yards: &[WebYardSummary]) -> String {
    if yards.is_empty() {
        return "No Web Yards found.".to_owned();
    }
    yards
        .iter()
        .map(yard_detail)
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn yard_detail(yard: &WebYardSummary) -> String {
    format!(
        "Web Yard: {}\nURL: {}\nStatus: {:?}\nCurrent deploy: {}",
        yard.name,
        yard.url,
        yard.status,
        yard.current_deploy_id.as_deref().unwrap_or("none")
    )
}

fn deploy_lines(deploys: &[YardDeploySummary]) -> String {
    if deploys.is_empty() {
        return "No Web Yard deploys found.".to_owned();
    }
    deploys
        .iter()
        .map(|deploy| {
            format!(
                "{}{}\t{}\t{}\t{}\t{} files\t{} bytes\t{}",
                deploy.id,
                if deploy.is_current { " *" } else { "" },
                deploy_status(deploy.status),
                deploy.created_at,
                deploy
                    .finalised_at
                    .map_or_else(|| "not finalised".to_owned(), |value| value.to_string()),
                deploy.file_count,
                deploy.total_bytes,
                deploy.deployment_url
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn yard_mutation_output<'a>(
    yard: &'a WebYardSummary,
    response: &'a YardDeploymentResponse,
) -> YardMutationOutput<'a> {
    YardMutationOutput {
        yard: &yard.name,
        yard_url: &response.url,
        deployment_url: &response.deployment_url,
        deploy_id: &response.deploy_id,
        status: deploy_status(response.status),
    }
}

fn deployment_detail(response: &YardMutationOutput<'_>) -> String {
    format!(
        "Web Yard: {}\nURL: {}\nDeployment URL: {}\nDeploy: {}\nStatus: {}",
        response.yard,
        response.yard_url,
        response.deployment_url,
        response.deploy_id,
        response.status
    )
}

pub(super) const fn deploy_status(status: YardDeployStatus) -> &'static str {
    match status {
        YardDeployStatus::Uploading => "uploading",
        YardDeployStatus::Finalising => "finalising",
        YardDeployStatus::Live => "live",
        YardDeployStatus::Failed => "failed",
        YardDeployStatus::Superseded => "superseded",
        YardDeployStatus::Pruned => "pruned",
    }
}

fn unexpected_cursor() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::ProviderUnavailable,
        "Blobyard returned unsupported Web Yard pagination. Try again shortly.",
    )
}

fn ensure_unpaginated<T>(page: &blobyard_api_client::Page<T>) -> Result<(), BlobyardError> {
    if page.next_cursor().is_some() {
        Err(unexpected_cursor())
    } else {
        Ok(())
    }
}

fn invalid_deploy_id() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InvalidRequest,
        "The deploy identifier isn't valid.",
    )
}

fn validate_deploy_id(deploy_id: Option<&str>) -> Result<(), BlobyardError> {
    if deploy_id.is_some_and(str::is_empty) {
        Err(invalid_deploy_id())
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "yards_tests.rs"]
mod tests;
