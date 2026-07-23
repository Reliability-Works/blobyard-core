use super::yards::select_yard;
use super::{Runner, command_result};
use crate::commands::EnvListArgs;
use blobyard_api_client::{
    ApiRequest, Endpoint, ListYardEnvironmentsQuery, YardEnvironmentKind, YardEnvironmentList,
    YardEnvironmentSummary,
};
use blobyard_core::{BlobyardError, Slug};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentListOutput<'a> {
    yard: &'a Slug,
    environments: &'a [YardEnvironmentSummary],
}

impl Runner {
    pub(super) async fn list_environments(
        &self,
        arguments: &EnvListArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let (yards, _request_id) = self.all_web_yards().await?;
        let selected = select_yard(&yards, arguments.name.as_deref())?;
        let request = ApiRequest::new(Endpoint::ListYardEnvironments).with_query(
            ListYardEnvironmentsQuery {
                yard_id: selected.id.clone(),
            }
            .into_query(),
        );
        let success = self.execute_authed::<YardEnvironmentList>(request).await?;
        let environments = &success.data().environments;
        command_result(
            &EnvironmentListOutput {
                yard: &selected.name,
                environments,
            },
            environment_lines(environments),
            success.request_id(),
        )
    }
}

fn environment_lines(environments: &[YardEnvironmentSummary]) -> String {
    if environments.is_empty() {
        return "No environments found.".to_owned();
    }
    environments
        .iter()
        .map(|environment| {
            format!(
                "{}\t{}\tcreated {}\t{}",
                environment.name,
                environment_kind(environment.kind),
                environment.created_at,
                environment.id
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const fn environment_kind(kind: YardEnvironmentKind) -> &'static str {
    match kind {
        YardEnvironmentKind::Production => "production",
        YardEnvironmentKind::Staging => "staging",
        YardEnvironmentKind::Preview => "preview",
    }
}

#[cfg(test)]
#[path = "environments_tests.rs"]
mod tests;
