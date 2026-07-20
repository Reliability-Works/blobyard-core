use crate::{api::AppState, auth::hash, error::ApiError};
use axum::http::HeaderMap;
use blobyard_api_client::RequestUploadRequest;
use blobyard_contract::{
    NewUploadReservation, ObjectSource, ProjectRecord, RepositoryError, ReservationState,
    ReservationStrategy, UploadReservationRecord, WorkspaceRecord,
};
use blobyard_core::{GeneratedSecretKind, SecretString};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub(crate) const GRANT_LIFETIME_MS: u64 = 15 * 60 * 1_000;
pub(crate) const SINGLE_UPLOAD_LIMIT_BYTES: u64 = 100 * 1_024 * 1_024;
pub(crate) const MULTIPART_PART_BYTES: u64 = 16 * 1_024 * 1_024;
const MAX_MULTIPART_PARTS: u64 = 10_000;

pub(crate) fn reservation_input(
    request: &RequestUploadRequest,
    project: &ProjectRecord,
    upload_id: &str,
    capability: &SecretString,
    expires_at_ms: u64,
    source: ObjectSource,
) -> NewUploadReservation {
    let (strategy, part_size, part_count) = upload_strategy(request.size_bytes);
    NewUploadReservation {
        id: upload_id.to_owned(),
        project_id: project.id.clone(),
        object_path: request.path.clone(),
        filename: request.filename.clone(),
        content_type: request.content_type.clone(),
        expected_size: request.size_bytes,
        expected_checksum: request.checksum_sha256.clone(),
        storage_key: format!("versions/{}/{upload_id}", project.id),
        capability_hash: hash(capability.expose_secret()),
        expires_at_ms,
        created_at_ms: expires_at_ms.saturating_sub(GRANT_LIFETIME_MS),
        source,
        git_repository: request.git_repository.clone(),
        git_commit: request.git_commit.clone(),
        git_branch: request.git_branch.clone(),
        strategy,
        part_size,
        part_count,
    }
}

fn upload_strategy(size: u64) -> (ReservationStrategy, Option<u64>, Option<u32>) {
    if size <= SINGLE_UPLOAD_LIMIT_BYTES {
        return (ReservationStrategy::Single, None, None);
    }
    let count = size.div_ceil(MULTIPART_PART_BYTES);
    let count = u32::try_from(count.min(MAX_MULTIPART_PARTS + 1)).unwrap_or(u32::MAX);
    (
        ReservationStrategy::Multipart,
        Some(MULTIPART_PART_BYTES),
        Some(count),
    )
}

pub(crate) fn validate_upload_strategy(input: &NewUploadReservation) -> Result<(), ApiError> {
    if input
        .part_count
        .is_some_and(|count| u64::from(count) > MAX_MULTIPART_PARTS)
    {
        Err(ApiError::invalid_request())
    } else {
        Ok(())
    }
}

pub(crate) fn authorize_project_binding(
    principal: &blobyard_contract::LocalApiTokenRecord,
    project: &ProjectRecord,
) -> Result<(), ApiError> {
    if principal
        .project_id
        .as_ref()
        .is_none_or(|project_id| project_id == &project.id)
    {
        Ok(())
    } else {
        Err(ApiError::not_found())
    }
}

pub(crate) fn reserve_or_replay(
    state: &AppState,
    input: &NewUploadReservation,
    now: u64,
    expires_at_ms: u64,
) -> Result<UploadReservationRecord, ApiError> {
    match state.repository.reserve_upload(input) {
        Ok(record) => Ok(record),
        Err(RepositoryError::Conflict) => replay(state, input, now, expires_at_ms),
        Err(error) => Err(ApiError::from_repository(error)),
    }
}

fn replay(
    state: &AppState,
    input: &NewUploadReservation,
    now: u64,
    expires_at_ms: u64,
) -> Result<UploadReservationRecord, ApiError> {
    let record = match state.repository.upload_by_id(&input.id) {
        Ok(record) => record,
        Err(error) => return Err(ApiError::from_repository(error)),
    };
    if record.state != ReservationState::Requested || !same_request(&record, input) {
        return Err(ApiError::conflict());
    }
    if record.expires_at_ms <= now {
        if let Err(error) = state.repository.renew_upload(&record.id, expires_at_ms) {
            return Err(ApiError::from_repository(error));
        }
        state
            .repository
            .upload_by_id(&record.id)
            .map_err(ApiError::from_repository)
    } else {
        Ok(record)
    }
}

pub(crate) fn same_request(record: &UploadReservationRecord, input: &NewUploadReservation) -> bool {
    record.version.project_id == input.project_id
        && record.version.object_path == input.object_path
        && record.filename == input.filename
        && record.content_type == input.content_type
        && record.expected_size == input.expected_size
        && record.expected_checksum == input.expected_checksum
        && record.version.storage_key == input.storage_key
        && record.version.source == input.source
        && record.version.git_repository == input.git_repository
        && record.version.git_commit == input.git_commit
        && record.version.git_branch == input.git_branch
}

pub(crate) fn resolve_project(
    state: &AppState,
    workspace_id: &str,
    request: &RequestUploadRequest,
) -> Result<ProjectRecord, ApiError> {
    resolve_project_slugs(state, workspace_id, &request.workspace, &request.project)
}

pub(crate) fn resolve_project_slugs(
    state: &AppState,
    workspace_id: &str,
    workspace_slug: &blobyard_core::Slug,
    project_slug: &blobyard_core::Slug,
) -> Result<ProjectRecord, ApiError> {
    let workspace = state
        .repository
        .workspace_by_slug(workspace_slug)
        .map_err(ApiError::from_repository)?;
    if workspace.id != workspace_id {
        return Err(ApiError::not_found());
    }
    state
        .repository
        .project_by_slug(&workspace.id, project_slug)
        .map_err(ApiError::from_repository)
}

pub(crate) fn resolve_authorized_project(
    state: &AppState,
    principal: &blobyard_contract::LocalApiTokenRecord,
    workspace_slug: &blobyard_core::Slug,
    project_slug: &blobyard_core::Slug,
) -> Result<ProjectRecord, ApiError> {
    let project =
        resolve_project_slugs(state, &principal.workspace_id, workspace_slug, project_slug)?;
    authorize_project_binding(principal, &project)?;
    Ok(project)
}

pub(crate) fn authorize_reservation(
    state: &AppState,
    workspace_id: &str,
    reservation: &UploadReservationRecord,
) -> Result<ProjectRecord, ApiError> {
    state
        .repository
        .list_projects(workspace_id)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .find(|project| project.id == reservation.version.project_id)
        .ok_or_else(ApiError::not_found)
}

pub(crate) fn workspace_by_id(state: &AppState, id: &str) -> Result<WorkspaceRecord, ApiError> {
    state
        .repository
        .list_workspaces()
        .map_err(ApiError::from_repository)?
        .into_iter()
        .find(|workspace| workspace.id == id)
        .ok_or_else(ApiError::not_found)
}

pub(crate) fn stable_upload_id(principal_id: &str, idempotency: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(principal_id.as_bytes());
    digest.update([0]);
    digest.update(idempotency.as_bytes());
    format!("upload_{}", blobyard_core::hex_digest(&digest.finalize()))
}

pub(crate) fn stable_capability(
    state: &AppState,
    principal_id: &str,
    idempotency: &str,
) -> SecretString {
    let mut digest = Sha256::new();
    digest.update(state.capability_key.expose_secret().as_bytes());
    digest.update([0]);
    digest.update(principal_id.as_bytes());
    digest.update([0]);
    digest.update(idempotency.as_bytes());
    SecretString::from_generated_entropy(
        GeneratedSecretKind::UploadCapability,
        digest.finalize().into(),
    )
}

pub(crate) fn stable_part_capability(
    state: &AppState,
    upload_id: &str,
    part_number: u32,
) -> SecretString {
    let mut digest = Sha256::new();
    digest.update(state.capability_key.expose_secret().as_bytes());
    digest.update([0]);
    digest.update(upload_id.as_bytes());
    digest.update([0]);
    digest.update(part_number.to_le_bytes());
    SecretString::from_generated_entropy(
        GeneratedSecretKind::UploadCapability,
        digest.finalize().into(),
    )
}

pub(crate) fn transfer_url(
    public_origin: &str,
    path: &str,
    capability: &SecretString,
) -> Result<SecretString, ApiError> {
    SecretString::new(format!(
        "{public_origin}/{path}/{}",
        capability.expose_secret()
    ))
    .map_err(|_error| ApiError::internal())
}

pub(crate) fn idempotency_key(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(ApiError::invalid_request)
}

pub(crate) fn validate_field(value: &str) -> Result<(), ApiError> {
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err(ApiError::invalid_request())
    } else {
        Ok(())
    }
}

pub(crate) fn now_ms() -> Result<u64, ApiError> {
    now_ms_from(SystemTime::now())
}

fn now_ms_from(time: SystemTime) -> Result<u64, ApiError> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_error| ApiError::internal())?;
    ApiError::internal_result(u64::try_from(duration.as_millis()))
}

pub(crate) fn format_expiry(value: u64) -> Result<String, ApiError> {
    let nanos = i128::from(value) * 1_000_000;
    let formatted = OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .map_err(|_error| ApiError::internal())?
        .format(&Rfc3339);
    ApiError::internal_result(formatted)
}

pub(crate) const fn conceal_capability_error(error: RepositoryError) -> ApiError {
    match error {
        RepositoryError::NotFound | RepositoryError::Conflict | RepositoryError::InvalidInput => {
            ApiError::not_found()
        }
        RepositoryError::SchemaTooNew | RepositoryError::Unavailable => ApiError::internal(),
    }
}

#[cfg(test)]
#[path = "transfer_grants_tests.rs"]
mod tests;
