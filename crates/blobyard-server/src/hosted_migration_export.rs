use super::{HostedMigrationError, HostedMigrationOptions, projection::SourceObject};
use blobyard_api_client::{
    ApiClient, ApiClientConfig, ApiRequest, DownloadResponse, Endpoint, RequestDownloadRequest,
    ReqwestTransport,
};
use blobyard_core::{BlobyardUri, SecretString};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

#[path = "hosted_migration_export_io.rs"]
mod io;
#[path = "hosted_migration_export_manifest.rs"]
mod manifest;

#[cfg(test)]
use io::{checksum, fetch_bytes, signed_url};
use io::{
    download_artifact, download_datasets, fetch_client, fetch_object, temporary_directory,
    validate_grant,
};
use manifest::parse_index;
#[cfg(test)]
use manifest::{
    ExportPart, MAX_EXPORT_PART_BYTES, MAX_EXPORT_PARTS, REQUIRED_DATASETS, valid_checksum,
    validate_parts,
};

#[cfg(any(test, feature = "test-seams"))]
#[path = "hosted_migration_export_test_faults.rs"]
mod test_faults;

pub(super) struct DownloadedExport {
    pub(super) datasets: BTreeMap<String, Vec<Value>>,
    api: ApiClient,
    fetch: reqwest::Client,
    token: SecretString,
}

pub(super) struct DownloadedObjects {
    pub(super) _temporary: tempfile::TempDir,
    pub(super) paths: BTreeMap<String, std::path::PathBuf>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportRequestState {
    export_id: String,
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportState {
    artifact_count: usize,
    error_code: Option<String>,
    id: String,
    status: String,
}

pub(super) async fn download(
    options: &HostedMigrationOptions,
    token: SecretString,
) -> Result<DownloadedExport, HostedMigrationError> {
    let api = api_client(&options.source_url)?;
    let fetch = fetch_client()?;
    let state = request_export(&api, &token).await?;
    let ready = wait_for_export(&api, &token, options, &state.export_id).await?;
    let index_bytes = download_artifact(&api, &fetch, &token, &ready.id, 0, None).await?;
    let parts = parse_index(&index_bytes, ready.artifact_count)?;
    let datasets = download_datasets(&api, &fetch, &token, &ready.id, &parts).await?;
    Ok(DownloadedExport {
        datasets,
        api,
        fetch,
        token,
    })
}

pub(super) async fn download_objects(
    source: &DownloadedExport,
    objects: &[SourceObject],
) -> Result<DownloadedObjects, HostedMigrationError> {
    let temporary = temporary_directory().map_err(|_error| HostedMigrationError::Persistence)?;
    let mut paths = BTreeMap::new();
    for object in objects {
        let uri = BlobyardUri::from_str(&object.uri)
            .map_err(|_error| HostedMigrationError::InvalidExport)?;
        let response = source
            .api
            .execute::<DownloadResponse>(
                ApiRequest::new(Endpoint::RequestDownload)
                    .with_json(RequestDownloadRequest { uri }.into_json())
                    .with_bearer(source.token.clone()),
            )
            .await
            .map_err(|_error| HostedMigrationError::SourceApi)?
            .into_data();
        validate_grant(&response, object)?;
        let path = temporary.path().join(&object.version_id);
        fetch_object(&source.fetch, &response.download_url, object, &path).await?;
        if paths.insert(object.version_id.clone(), path).is_some() {
            return Err(HostedMigrationError::InvalidExport);
        }
    }
    Ok(DownloadedObjects {
        _temporary: temporary,
        paths,
    })
}

fn api_client(source_url: &str) -> Result<ApiClient, HostedMigrationError> {
    let config =
        ApiClientConfig::new(source_url).map_err(|_error| HostedMigrationError::InvalidInput)?;
    let transport = build_transport(config).map_err(|_error| HostedMigrationError::SourceApi)?;
    Ok(ApiClient::new(Arc::new(transport)))
}

fn build_transport(
    config: ApiClientConfig,
) -> Result<ReqwestTransport, blobyard_core::BlobyardError> {
    #[cfg(any(test, feature = "test-seams"))]
    if test_faults::active(test_faults::ExportFault::ApiTransport) {
        return Err(blobyard_core::BlobyardError::from_code(
            blobyard_core::ErrorCode::ProviderUnavailable,
        ));
    }
    ReqwestTransport::new(config)
}

async fn request_export(
    api: &ApiClient,
    token: &SecretString,
) -> Result<ExportRequestState, HostedMigrationError> {
    let state = api
        .execute::<ExportRequestState>(
            ApiRequest::new(Endpoint::RequestAccountExport)
                .with_json(json!({}))
                .with_generated_idempotency_key()
                .with_bearer(token.clone()),
        )
        .await
        .map_err(|_error| HostedMigrationError::SourceApi)?
        .into_data();
    if matches!(state.status.as_str(), "queued" | "running") && !state.export_id.is_empty() {
        Ok(state)
    } else {
        Err(HostedMigrationError::InvalidExport)
    }
}

async fn wait_for_export(
    api: &ApiClient,
    token: &SecretString,
    options: &HostedMigrationOptions,
    expected_id: &str,
) -> Result<ExportState, HostedMigrationError> {
    for _attempt in 0..options.poll_limit {
        let state = api
            .execute::<ExportState>(
                ApiRequest::new(Endpoint::GetAccountExport).with_bearer(token.clone()),
            )
            .await
            .map_err(|_error| HostedMigrationError::SourceApi)?
            .into_data();
        if state.id != expected_id || state.error_code.is_some() {
            return Err(HostedMigrationError::InvalidExport);
        }
        match state.status.as_str() {
            "ready" => return Ok(state),
            "queued" | "running" => tokio::time::sleep(options.poll_interval).await,
            "failed" | "expired" => return Err(HostedMigrationError::SourceApi),
            _ => return Err(HostedMigrationError::InvalidExport),
        }
    }
    Err(HostedMigrationError::SourceApi)
}

#[cfg(test)]
#[path = "hosted_migration_export_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "hosted_migration_export_http_tests.rs"]
mod http_tests;

#[cfg(test)]
#[path = "hosted_migration_export_http_fixture.rs"]
mod http_fixture;

#[cfg(test)]
#[path = "hosted_migration_export_object_http_tests.rs"]
mod object_http_tests;

#[cfg(test)]
#[path = "hosted_migration_export_success_tests.rs"]
mod success_tests;
