use super::discovery::UploadFile;
use super::file_facts::FileFacts;
use super::identifiers::idempotency_digest;
use super::provenance::GitProvenance;
use super::resume::{ResumeState, load, remove, save};
use super::upload_math::validate_grants;
use crate::runner::Runner;
use blobyard_api_client::{
    AbortUploadRequest, ApiRequest, CompleteUploadRequest, CompleteUploadResponse, EmptyResponse,
    Endpoint, RequestUploadPartsRequest, RequestUploadPartsResponse, RequestUploadRequest,
    RequestUploadResponse, UploadStatusQuery, UploadStatusResponse,
};
use blobyard_core::{BlobyardError, ErrorCode, Slug};
use std::path::Path;

impl Runner {
    pub(super) async fn reserve(
        &self,
        workspace: &Slug,
        project: &Slug,
        file: &UploadFile,
        facts: &FileFacts,
        provenance: &GitProvenance,
    ) -> Result<RequestUploadResponse, BlobyardError> {
        let body = RequestUploadRequest {
            workspace: workspace.clone(),
            project: project.clone(),
            path: file.logical_path.clone(),
            filename: file.filename.clone(),
            size_bytes: facts.size_bytes,
            checksum_sha256: facts.checksum_sha256.clone(),
            content_type: facts.content_type.clone(),
            git_repository: provenance.repository.clone(),
            git_commit: provenance.commit.clone(),
            git_branch: provenance.branch.clone(),
        };
        let size = facts.size_bytes.to_string();
        let digest = idempotency_digest(
            "upload",
            &[
                workspace.as_str(),
                project.as_str(),
                &file.logical_path,
                &size,
                &facts.fingerprint,
            ],
        );
        let request = ApiRequest::new(Endpoint::RequestUpload)
            .with_json(body.into_json())
            .with_deterministic_idempotency_key(digest);
        self.execute_authed::<RequestUploadResponse>(request)
            .await
            .map(blobyard_api_client::ApiSuccess::into_data)
    }

    pub(super) async fn usable_resume(
        &self,
        path: &Path,
        facts: &FileFacts,
    ) -> Result<Option<ResumeState>, BlobyardError> {
        let Some(mut state) = load(path)? else {
            return Ok(None);
        };
        if !state.matches(&facts.fingerprint) {
            remove(path)?;
            return Ok(None);
        }
        let request = ApiRequest::new(Endpoint::UploadStatus).with_query(
            UploadStatusQuery {
                upload_id: state.upload_id().to_owned(),
            }
            .into_query(),
        );
        match self.execute_authed::<UploadStatusResponse>(request).await {
            Ok(status) => {
                state.retain_server_parts(&status.data().completed_parts);
                save(path, &state)?;
                Ok(Some(state))
            }
            Err(error) if error.code() == ErrorCode::NotFound => {
                remove(path)?;
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) async fn request_part_grants(
        &self,
        upload_id: &str,
        part_numbers: &[u32],
    ) -> Result<Vec<blobyard_api_client::UploadPartGrant>, BlobyardError> {
        let body = RequestUploadPartsRequest {
            upload_id: upload_id.to_owned(),
            part_numbers: part_numbers.to_vec(),
        };
        let request = ApiRequest::new(Endpoint::RequestUploadParts).with_json(body.into_json());
        let response = self
            .execute_authed::<RequestUploadPartsResponse>(request)
            .await?
            .into_data();
        validate_grants(part_numbers, response.parts)
    }

    pub(super) async fn complete(
        &self,
        upload_id: &str,
        parts: Vec<blobyard_api_client::CompletedPart>,
    ) -> Result<(CompleteUploadResponse, String), BlobyardError> {
        let body = CompleteUploadRequest {
            upload_id: upload_id.to_owned(),
            parts,
        };
        let request = ApiRequest::new(Endpoint::CompleteUpload).with_json(body.into_json());
        let success = self
            .execute_authed::<CompleteUploadResponse>(request)
            .await?;
        let request_id = success.request_id().to_owned();
        Ok((success.into_data(), request_id))
    }

    pub(super) async fn abort(&self, upload_id: &str) -> Result<(), BlobyardError> {
        let body = AbortUploadRequest {
            upload_id: upload_id.to_owned(),
        };
        let request = ApiRequest::new(Endpoint::AbortUpload).with_json(body.into_json());
        self.execute_authed::<EmptyResponse>(request)
            .await
            .map(|_success| ())
    }
}
