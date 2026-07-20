use super::manifest::{ExportPart, MAX_EXPORT_PART_BYTES, REQUIRED_DATASETS};
#[cfg(any(test, feature = "test-seams"))]
use super::test_faults;
use super::{HostedMigrationError, SourceObject};
use blobyard_api_client::{ApiClient, ApiRequest, DownloadResponse, Endpoint, host_is_loopback};
use blobyard_core::SecretString;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::time::Duration;
use url::Url;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactGrant {
    download_url: SecretString,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DatasetFile {
    dataset: String,
    records: Vec<Value>,
}

pub(super) fn temporary_directory() -> Result<tempfile::TempDir, std::io::Error> {
    #[cfg(any(test, feature = "test-seams"))]
    if test_faults::active(test_faults::ExportFault::TemporaryDirectory) {
        return Err(std::io::Error::other("fixture temporary directory failure"));
    }
    tempfile::tempdir()
}

pub(super) fn fetch_client() -> Result<reqwest::Client, HostedMigrationError> {
    #[cfg(any(test, feature = "test-seams"))]
    if test_faults::active(test_faults::ExportFault::FetchClient) {
        return Err(HostedMigrationError::SourceDownload);
    }
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_mins(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("blobyard-server/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_error| HostedMigrationError::SourceDownload)
}

pub(super) async fn download_datasets(
    api: &ApiClient,
    fetch: &reqwest::Client,
    token: &SecretString,
    export_id: &str,
    parts: &[ExportPart],
) -> Result<BTreeMap<String, Vec<Value>>, HostedMigrationError> {
    let mut datasets = BTreeMap::<String, Vec<Value>>::new();
    for part in parts
        .iter()
        .filter(|part| REQUIRED_DATASETS.contains(&part.dataset.as_str()))
    {
        let bytes =
            download_artifact(api, fetch, token, export_id, part.part_number, Some(part)).await?;
        let file = serde_json::from_slice::<DatasetFile>(&bytes)
            .map_err(|_error| HostedMigrationError::InvalidExport)?;
        if file.dataset != part.dataset {
            return Err(HostedMigrationError::InvalidExport);
        }
        datasets
            .entry(file.dataset)
            .or_default()
            .extend(file.records);
    }
    if REQUIRED_DATASETS
        .iter()
        .all(|dataset| datasets.contains_key(*dataset))
    {
        Ok(datasets)
    } else {
        Err(HostedMigrationError::InvalidExport)
    }
}

pub(super) async fn download_artifact(
    api: &ApiClient,
    fetch: &reqwest::Client,
    token: &SecretString,
    export_id: &str,
    part_number: u32,
    expected: Option<&ExportPart>,
) -> Result<Vec<u8>, HostedMigrationError> {
    let grant = api
        .execute::<ArtifactGrant>(
            ApiRequest::new(Endpoint::DownloadAccountExport)
                .with_json(json!({ "exportId": export_id, "partNumber": part_number }))
                .with_bearer(token.clone()),
        )
        .await
        .map_err(|_error| HostedMigrationError::SourceApi)?
        .into_data();
    let maximum = expected.map_or(MAX_EXPORT_PART_BYTES, |part| part.byte_size);
    let bytes = fetch_bytes(fetch, &grant.download_url, maximum).await?;
    if let Some(part) = expected
        && (bytes.len() as u64 != part.byte_size || checksum(&bytes) != part.checksum_sha256)
    {
        return Err(HostedMigrationError::Integrity);
    }
    Ok(bytes)
}

pub(super) async fn fetch_bytes(
    client: &reqwest::Client,
    secret_url: &SecretString,
    maximum: u64,
) -> Result<Vec<u8>, HostedMigrationError> {
    let url = signed_url(secret_url)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_error| HostedMigrationError::SourceDownload)?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > maximum)
    {
        return Err(HostedMigrationError::SourceDownload);
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_error| HostedMigrationError::SourceDownload)?;
        if (bytes.len() as u64).saturating_add(chunk.len() as u64) > maximum {
            return Err(HostedMigrationError::SourceDownload);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(super) fn validate_grant(
    response: &DownloadResponse,
    object: &SourceObject,
) -> Result<(), HostedMigrationError> {
    if response.size_bytes == object.size && response.checksum_sha256 == object.checksum {
        Ok(())
    } else {
        Err(HostedMigrationError::Integrity)
    }
}

pub(super) async fn fetch_object(
    client: &reqwest::Client,
    secret_url: &SecretString,
    object: &SourceObject,
    path: &std::path::Path,
) -> Result<(), HostedMigrationError> {
    let response = client
        .get(signed_url(secret_url)?)
        .send()
        .await
        .map_err(|_error| HostedMigrationError::SourceDownload)?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length != object.size)
    {
        return Err(HostedMigrationError::SourceDownload);
    }
    let mut file = create_file(path).map_err(|_error| HostedMigrationError::Persistence)?;
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_error| HostedMigrationError::SourceDownload)?;
        size = size.saturating_add(chunk.len() as u64);
        if size > object.size {
            return Err(HostedMigrationError::Integrity);
        }
        digest.update(&chunk);
        write_chunk(&mut file, &chunk).map_err(|_error| HostedMigrationError::Persistence)?;
    }
    flush_file(&mut file).map_err(|_error| HostedMigrationError::Persistence)?;
    sync_file(&file).map_err(|_error| HostedMigrationError::Persistence)?;
    if size == object.size && blobyard_core::hex_digest(&digest.finalize()) == object.checksum {
        Ok(())
    } else {
        Err(HostedMigrationError::Integrity)
    }
}

fn create_file(path: &std::path::Path) -> Result<std::fs::File, std::io::Error> {
    #[cfg(any(test, feature = "test-seams"))]
    if test_faults::active(test_faults::ExportFault::CreateFile) {
        return Err(std::io::Error::other("fixture create failure"));
    }
    std::fs::File::create(path)
}

fn write_chunk(file: &mut std::fs::File, bytes: &[u8]) -> Result<(), std::io::Error> {
    #[cfg(any(test, feature = "test-seams"))]
    if test_faults::active(test_faults::ExportFault::WriteFile) {
        return Err(std::io::Error::other("fixture write failure"));
    }
    file.write_all(bytes)
}

fn flush_file(file: &mut std::fs::File) -> Result<(), std::io::Error> {
    #[cfg(any(test, feature = "test-seams"))]
    if test_faults::active(test_faults::ExportFault::FlushFile) {
        return Err(std::io::Error::other("fixture flush failure"));
    }
    file.flush()
}

fn sync_file(file: &std::fs::File) -> Result<(), std::io::Error> {
    #[cfg(any(test, feature = "test-seams"))]
    if test_faults::active(test_faults::ExportFault::SyncFile) {
        return Err(std::io::Error::other("fixture sync failure"));
    }
    file.sync_all()
}

pub(super) fn signed_url(secret_url: &SecretString) -> Result<Url, HostedMigrationError> {
    let url = Url::parse(secret_url.expose_secret())
        .map_err(|_error| HostedMigrationError::SourceDownload)?;
    let allowed_scheme = url.scheme() == "https"
        || (url.scheme() == "http" && url.host().as_ref().is_some_and(host_is_loopback));
    if allowed_scheme
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
    {
        Ok(url)
    } else {
        Err(HostedMigrationError::SourceDownload)
    }
}

pub(super) fn checksum(bytes: &[u8]) -> String {
    blobyard_core::hex_digest(&Sha256::digest(bytes))
}
