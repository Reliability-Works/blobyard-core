use super::yards::deploy_status;
use blobyard_api_client::{StartYardDeployResponse, YardDeploymentResponse};
use blobyard_core::BlobyardError;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeployOutput {
    yard: String,
    yard_url: String,
    deployment_url: String,
    deploy_id: String,
    status: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeployBatchOutput {
    pub(super) results: Vec<DeployItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeployItem {
    yard: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    yard_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployment_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deploy_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<DeployItemError>,
}

#[derive(Debug, Serialize)]
struct DeployItemError {
    code: &'static str,
    message: String,
}

impl DeployItem {
    pub(super) fn success(output: DeployOutput) -> Self {
        Self {
            yard: output.yard,
            ok: true,
            yard_url: Some(output.yard_url),
            deployment_url: Some(output.deployment_url),
            deploy_id: Some(output.deploy_id),
            status: Some(output.status),
            error: None,
        }
    }

    pub(super) fn failure(yard: String, error: &BlobyardError) -> Self {
        Self {
            yard,
            ok: false,
            yard_url: None,
            deployment_url: None,
            deploy_id: None,
            status: Some("failed"),
            error: Some(DeployItemError {
                code: error.code().as_str(),
                message: error.message().to_owned(),
            }),
        }
    }
}

pub(super) fn deploy_output(
    started: &StartYardDeployResponse,
    response: &YardDeploymentResponse,
) -> DeployOutput {
    DeployOutput {
        yard: started.yard_name.to_string(),
        yard_url: response.url.clone(),
        deployment_url: response.deployment_url.clone(),
        deploy_id: response.deploy_id.clone(),
        status: deploy_status(response.status),
    }
}

pub(super) fn detail(output: &DeployOutput) -> String {
    format!(
        "Web Yard: {}\nURL: {}\nDeployment URL: {}\nDeploy: {}\nStatus: {}",
        output.yard, output.yard_url, output.deployment_url, output.deploy_id, output.status
    )
}

pub(super) fn batch_detail(results: &[DeployItem]) -> String {
    results
        .iter()
        .map(|result| {
            result.error.as_ref().map_or_else(
                || {
                    format!(
                        "Web Yard: {}\nURL: {}\nDeployment URL: {}\nDeploy: {}\nStatus: {}",
                        result.yard,
                        result.yard_url.as_deref().unwrap_or("unknown"),
                        result.deployment_url.as_deref().unwrap_or("unknown"),
                        result.deploy_id.as_deref().unwrap_or("unknown"),
                        result.status.unwrap_or("unknown")
                    )
                },
                |error| {
                    format!(
                        "Web Yard: {}\nStatus: failed\nError: [{}] {}",
                        result.yard, error.code, error.message
                    )
                },
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
