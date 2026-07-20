use super::http::SignedTransferClient;
use crate::commands::DownloadArgs;
use crate::runner::{Runner, command_result};
use blobyard_api_client::{DownloadResponse, Endpoint, RequestDownloadRequest};
use blobyard_core::{BlobyardError, BlobyardUri, ErrorCode};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadOutput {
    uri: BlobyardUri,
    output: String,
    filename: String,
    size_bytes: u64,
    checksum_sha256: String,
}

impl Runner {
    pub(crate) async fn download(
        &self,
        arguments: &DownloadArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let uri = BlobyardUri::from_str(&arguments.uri).map_err(|_error| invalid_uri())?;
        validate_destination(&arguments.output, arguments.force)?;
        let response = self
            .execute_authed::<DownloadResponse>(
                self.mutation(Endpoint::RequestDownload)
                    .with_json(RequestDownloadRequest { uri: uri.clone() }.into_json()),
            )
            .await?;
        let request_id = response.request_id().to_owned();
        let grant = response.into_data();
        let temporary = temporary_path(&arguments.output)?;
        let transfer = SignedTransferClient::new();
        let progress = self
            .transfer_progress
            .start(&grant.filename, grant.size_bytes);
        let measured = match transfer
            .download(&grant.download_url, &temporary, &progress)
            .await
        {
            Ok(measured) => measured,
            Err(error) => {
                progress.finish_and_clear();
                cleanup(&temporary);
                return Err(error);
            }
        };
        progress.finish_and_clear();
        if measured.size_bytes != grant.size_bytes
            || measured.checksum_sha256 != grant.checksum_sha256
        {
            cleanup(&temporary);
            return Err(BlobyardError::from_code(ErrorCode::ChecksumMismatch));
        }
        place_download(&temporary, &arguments.output, arguments.force)?;
        let output = DownloadOutput {
            uri,
            output: arguments.output.to_string_lossy().into_owned(),
            filename: grant.filename,
            size_bytes: measured.size_bytes,
            checksum_sha256: measured.checksum_sha256,
        };
        let human = format!(
            "Downloaded {} to {} ({} bytes).",
            output.uri, output.output, output.size_bytes
        );
        command_result(&output, human, &request_id)
    }
}

pub(super) fn validate_destination(path: &Path, force: bool) -> Result<(), BlobyardError> {
    if path.is_dir() {
        return Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            "The download destination must be a file path.",
        ));
    }
    if path.exists() && !force {
        Err(BlobyardError::new(
            ErrorCode::Conflict,
            "The download destination already exists. Use --force to replace it.",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn temporary_path(destination: &Path) -> Result<PathBuf, BlobyardError> {
    let parent = destination
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(write_error());
    }
    let name = format!(".blobyard-download-{}.tmp", uuid::Uuid::new_v4());
    Ok(parent.join(name))
}

pub(super) fn place_download(
    temporary: &Path,
    destination: &Path,
    force: bool,
) -> Result<(), BlobyardError> {
    #[cfg(windows)]
    if force && destination.exists() {
        std::fs::remove_file(destination).map_err(|_error| write_error())?;
    }
    #[cfg(not(windows))]
    let _ = force;
    std::fs::rename(temporary, destination).map_err(|_error| {
        cleanup(temporary);
        write_error()
    })
}

fn cleanup(path: &Path) {
    let _ignored = std::fs::remove_file(path);
}

fn invalid_uri() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::InvalidRequest,
        "The Blobyard URI isn't valid. Check it and try again.",
    )
}

fn write_error() -> BlobyardError {
    BlobyardError::new(
        ErrorCode::StorageError,
        "Blobyard couldn't write the download. Check the destination and try again.",
    )
}
