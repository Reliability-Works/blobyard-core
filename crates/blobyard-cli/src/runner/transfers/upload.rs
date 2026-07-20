use super::discovery::{UploadFile, discover};
use super::file_facts::{FileFacts, inspect};
use super::http::SignedTransferClient;
use super::provenance::{GitProvenance, discover as discover_provenance};
use super::resume::{ResumeState, remove, save, state_path};
use super::upload_math::{contract_error, multipart_state, part_range, total_parts};
use crate::commands::UploadArgs;
use crate::runner::{Runner, command_result};
use blobyard_api_client::{CompleteUploadResponse, RequestUploadResponse, UploadStrategy};
use blobyard_core::{BlobyardError, Slug};
use futures_util::{StreamExt, stream};
use indicatif::ProgressBar;
use serde::Serialize;
use std::path::Path;

const PART_CONCURRENCY: usize = 4;
const GRANT_BATCH_SIZE: usize = 16;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadOutput {
    files: Vec<CompleteUploadResponse>,
}

struct FreshUpload<'a> {
    facts: &'a FileFacts,
    file: &'a UploadFile,
    progress: &'a ProgressBar,
    project: &'a Slug,
    provenance: &'a GitProvenance,
    resume_path: &'a Path,
    transfer: &'a SignedTransferClient,
    workspace: &'a Slug,
}

impl Runner {
    pub(crate) async fn upload(
        &self,
        arguments: &UploadArgs,
    ) -> Result<crate::CommandResult, BlobyardError> {
        let (completed, request_id) = self.upload_files(arguments).await?;
        let human = upload_lines(&completed);
        command_result(
            &UploadOutput { files: completed },
            human,
            request_id.as_str(),
        )
    }

    pub(in crate::runner) async fn upload_files(
        &self,
        arguments: &UploadArgs,
    ) -> Result<(Vec<CompleteUploadResponse>, String), BlobyardError> {
        let (workspace, project) = self.scope()?;
        let files = discover(arguments)?;
        let provenance = discover_provenance(&arguments.source);
        let transfer = SignedTransferClient::new();
        let mut completed = Vec::with_capacity(files.len());
        let mut request_id = String::new();
        for file in files {
            let result = self
                .upload_file(&transfer, &workspace, &project, &file, &provenance)
                .await?;
            request_id = result.1;
            completed.push(result.0);
        }
        Ok((completed, request_id))
    }

    pub(super) async fn upload_file(
        &self,
        transfer: &SignedTransferClient,
        workspace: &Slug,
        project: &Slug,
        file: &UploadFile,
        provenance: &GitProvenance,
    ) -> Result<(CompleteUploadResponse, String), BlobyardError> {
        let facts = inspect(&file.source).await?;
        let resume_path = state_path(&file.source, &file.logical_path);
        let progress = self
            .transfer_progress
            .start(&file.logical_path, facts.size_bytes);
        let result = match self.usable_resume(&resume_path, &facts).await {
            Ok(Some(state)) => {
                self.multipart_upload(transfer, file, &facts, &resume_path, state, &progress)
                    .await
            }
            Ok(None) => {
                self.fresh_upload(FreshUpload {
                    facts: &facts,
                    file,
                    progress: &progress,
                    project,
                    provenance,
                    resume_path: &resume_path,
                    transfer,
                    workspace,
                })
                .await
            }
            Err(error) => Err(error),
        };
        progress.finish_and_clear();
        result
    }

    async fn fresh_upload(
        &self,
        upload: FreshUpload<'_>,
    ) -> Result<(CompleteUploadResponse, String), BlobyardError> {
        let reservation = self
            .reserve(
                upload.workspace,
                upload.project,
                upload.file,
                upload.facts,
                upload.provenance,
            )
            .await?;
        match reservation.strategy {
            UploadStrategy::Single => {
                self.single_upload(
                    upload.transfer,
                    upload.file,
                    upload.facts,
                    reservation,
                    upload.progress,
                )
                .await
            }
            UploadStrategy::Multipart => {
                let state = multipart_state(&reservation, upload.facts)?;
                save(upload.resume_path, &state)?;
                self.multipart_upload(
                    upload.transfer,
                    upload.file,
                    upload.facts,
                    upload.resume_path,
                    state,
                    upload.progress,
                )
                .await
            }
        }
    }

    async fn single_upload(
        &self,
        transfer: &SignedTransferClient,
        file: &UploadFile,
        facts: &FileFacts,
        reservation: RequestUploadResponse,
        progress: &ProgressBar,
    ) -> Result<(CompleteUploadResponse, String), BlobyardError> {
        let url = reservation.upload_url.as_ref().ok_or_else(contract_error)?;
        if let Err(error) = transfer
            .put_file(
                url,
                &file.source,
                facts.size_bytes,
                &reservation.headers,
                progress,
            )
            .await
        {
            let _ignored = self.abort(&reservation.upload_id).await;
            return Err(error);
        }
        self.complete(&reservation.upload_id, Vec::new()).await
    }

    pub(super) async fn multipart_upload(
        &self,
        transfer: &SignedTransferClient,
        file: &UploadFile,
        facts: &FileFacts,
        resume_path: &Path,
        mut state: ResumeState,
        progress: &ProgressBar,
    ) -> Result<(CompleteUploadResponse, String), BlobyardError> {
        let total_parts = total_parts(facts.size_bytes, state.part_size_bytes())?;
        progress.set_position(state.completed_bytes(facts.size_bytes));
        for batch in state.pending(total_parts).chunks(GRANT_BATCH_SIZE) {
            let grants = self.request_part_grants(state.upload_id(), batch).await?;
            let part_size = state.part_size_bytes();
            let total_size = facts.size_bytes;
            let mut uploads = stream::iter(grants.into_iter().map(|grant| {
                upload_grant(
                    transfer.clone(),
                    file.source.clone(),
                    grant,
                    part_size,
                    total_size,
                    progress.clone(),
                )
            }))
            .buffer_unordered(PART_CONCURRENCY);
            while let Some(result) = uploads.next().await {
                let (number, etag) = result?;
                state.record(number, etag);
                save(resume_path, &state)?;
            }
        }
        let result = self
            .complete(state.upload_id(), state.completed_parts())
            .await?;
        let _cleanup = remove(resume_path);
        Ok(result)
    }
}

async fn upload_grant(
    client: SignedTransferClient,
    source: std::path::PathBuf,
    grant: blobyard_api_client::UploadPartGrant,
    part_size: u64,
    total_size: u64,
    progress: ProgressBar,
) -> Result<(u32, String), BlobyardError> {
    let (offset, size) = part_range(grant.part_number, part_size, total_size);
    client
        .put_part(&grant.upload_url, &source, offset, size, &progress)
        .await
        .map(|etag| (grant.part_number, etag))
}

fn upload_lines(completed: &[CompleteUploadResponse]) -> String {
    completed
        .iter()
        .map(|item| format!("Uploaded {} ({} bytes).", item.uri, item.size_bytes))
        .collect::<Vec<_>>()
        .join("\n")
}
