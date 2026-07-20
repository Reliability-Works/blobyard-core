use crate::{
    api::AppState,
    error::ApiError,
    inbox_upload_auth::InboxGuest,
    response::{Success, success},
    transfer_grants as grants,
};
use axum::Json;
use blobyard_api_client::{
    AbortUploadRequest, CompleteUploadRequest, CompleteUploadResponse, EmptyResponse,
    RequestUploadRequest, RequestUploadResponse, UploadStatusResponse,
};
use blobyard_contract::{
    AuditValue, NewAuditEvent, NewInboxUpload, ObjectSource, ProjectRecord, RepositoryError,
    ReservationState, UploadReservationRecord, WorkspaceRecord,
};

pub(crate) fn issue_at(
    state: &AppState,
    guest: &InboxGuest,
    request: &RequestUploadRequest,
    idempotency: &str,
    now: u64,
) -> Result<Json<Success<RequestUploadResponse>>, ApiError> {
    let workspace = workspace(state, &guest.inbox.workspace_id)?;
    let project = project(state, &guest.inbox)?;
    let request = scoped_request(request, &workspace, &project)?;
    let expires_at_ms = now
        .checked_add(grants::GRANT_LIFETIME_MS)
        .map(|value| value.min(guest.inbox.expires_at_ms))
        .ok_or_else(ApiError::internal)?;
    let identity = format!("{}:{}", guest.inbox.id, guest.fingerprint_hash);
    let upload_id = grants::stable_upload_id(&identity, idempotency);
    let capability = grants::stable_capability(state, &identity, idempotency);
    let upload_url = grants::transfer_url(&state.public_origin, "transfers/uploads", &capability)?;
    let mut input = grants::reservation_input(
        &request,
        &project,
        &upload_id,
        &capability,
        expires_at_ms,
        ObjectSource::Inbox,
    );
    input.created_at_ms = now;
    grants::validate_upload_strategy(&input)?;
    let principal = NewInboxUpload {
        capability_hash: guest.capability_hash.clone(),
        fingerprint_hash: guest.fingerprint_hash.clone(),
        now_ms: now,
    };
    let reservation = reserve_or_replay(state, guest, &principal, &input, now, expires_at_ms)?;
    let reservation = crate::transfer_multipart::ensure_provider(state, reservation)?;
    crate::transfers::upload_response(
        upload_id,
        upload_url,
        reservation.strategy,
        reservation.part_size,
        reservation.expires_at_ms,
    )
}

fn reserve_or_replay(
    state: &AppState,
    guest: &InboxGuest,
    principal: &NewInboxUpload,
    input: &blobyard_contract::NewUploadReservation,
    now: u64,
    expires_at_ms: u64,
) -> Result<UploadReservationRecord, ApiError> {
    match state.repository.reserve_inbox_upload(principal, input) {
        Ok(record) => Ok(record),
        Err(RepositoryError::Conflict) => replay(state, guest, input, now, expires_at_ms),
        Err(error) => Err(ApiError::from_repository(error)),
    }
}

fn replay(
    state: &AppState,
    guest: &InboxGuest,
    input: &blobyard_contract::NewUploadReservation,
    now: u64,
    expires_at_ms: u64,
) -> Result<UploadReservationRecord, ApiError> {
    let record = reservation(state, guest, &input.id, now)?;
    if record.state != ReservationState::Requested || !grants::same_request(&record, input) {
        return Err(ApiError::conflict());
    }
    if record.expires_at_ms <= now {
        state
            .repository
            .renew_upload(&record.id, expires_at_ms)
            .map_err(ApiError::from_repository)?;
        reservation(state, guest, &record.id, now)
    } else {
        Ok(record)
    }
}

pub(crate) fn reservation(
    state: &AppState,
    guest: &InboxGuest,
    upload_id: &str,
    now: u64,
) -> Result<UploadReservationRecord, ApiError> {
    state
        .repository
        .inbox_upload_by_id(&guest.capability_hash, upload_id, now)
        .map_err(ApiError::concealed_capability)
}

pub(crate) fn complete_at(
    state: &AppState,
    guest: &InboxGuest,
    request: &CompleteUploadRequest,
    now: u64,
) -> Result<Json<Success<CompleteUploadResponse>>, ApiError> {
    let reservation = reservation(state, guest, &request.upload_id, now)?;
    let workspace = workspace(state, &guest.inbox.workspace_id)?;
    let project = project(state, &guest.inbox)?;
    crate::transfer_multipart::complete(state, &reservation, &request.parts)?;
    let version = state
        .repository
        .complete_inbox_upload(
            &guest.capability_hash,
            &request.upload_id,
            now,
            &completion_event(guest, &reservation, now),
        )
        .map_err(ApiError::concealed_capability)?;
    crate::transfers_operations::completion_response(workspace.slug, project.slug, version)
}

pub(crate) fn abort_at(
    state: &AppState,
    guest: &InboxGuest,
    request: &AbortUploadRequest,
    now: u64,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let reservation = reservation(state, guest, &request.upload_id, now)?;
    crate::transfer_multipart::abort_storage(state, &reservation)?;
    state
        .repository
        .abort_inbox_upload(&guest.capability_hash, &request.upload_id, now)
        .map_err(ApiError::concealed_capability)?;
    Ok(success(EmptyResponse::default()))
}

pub(crate) fn status_at(
    state: &AppState,
    guest: &InboxGuest,
    upload_id: &str,
    now: u64,
) -> Result<Json<Success<UploadStatusResponse>>, ApiError> {
    let reservation = reservation(state, guest, upload_id, now)?;
    crate::transfers::status_response(state, &reservation, now)
}

fn scoped_request(
    request: &RequestUploadRequest,
    workspace: &WorkspaceRecord,
    project: &ProjectRecord,
) -> Result<RequestUploadRequest, ApiError> {
    let filename = sanitize_filename(&request.filename);
    grants::validate_field(&request.content_type)?;
    let path = format!("inbox/{filename}");
    Ok(RequestUploadRequest {
        workspace: workspace.slug.clone(),
        project: project.slug.clone(),
        path,
        filename,
        size_bytes: request.size_bytes,
        checksum_sha256: request.checksum_sha256.clone(),
        content_type: request.content_type.clone(),
        git_repository: None,
        git_commit: None,
        git_branch: None,
    })
}

fn sanitize_filename(input: &str) -> String {
    let basename = input.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut safe = String::with_capacity(basename.len().min(128));
    let mut separator = false;
    for character in basename.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
            if separator && !safe.is_empty() && safe.len() < 128 {
                safe.push('_');
            }
            separator = false;
            if safe.len() < 128 {
                safe.push(character);
            }
        } else {
            separator = true;
        }
    }
    let trimmed = safe.trim_matches(['.', '-', '_']);
    if trimmed.is_empty() {
        "file".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn project(
    state: &AppState,
    inbox: &blobyard_contract::InboxRecord,
) -> Result<ProjectRecord, ApiError> {
    state
        .repository
        .list_projects(&inbox.workspace_id)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .find(|project| project.id == inbox.project_id)
        .ok_or_else(ApiError::not_found)
}

fn workspace(state: &AppState, id: &str) -> Result<WorkspaceRecord, ApiError> {
    grants::workspace_by_id(state, id)
}

fn completion_event(
    guest: &InboxGuest,
    reservation: &UploadReservationRecord,
    now: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{}", uuid::Uuid::new_v4().simple()),
        workspace_id: guest.inbox.workspace_id.clone(),
        actor: guest.inbox.id.clone(),
        action: "inbox.uploaded".to_owned(),
        request_id: crate::error::request_id(),
        target_type: "object_version".to_owned(),
        metadata: vec![
            (
                "byteSize".to_owned(),
                AuditValue::Number(reservation.expected_size),
            ),
            ("source".to_owned(), AuditValue::String("inbox".to_owned())),
        ],
        created_at_ms: now,
    }
}

#[cfg(test)]
#[path = "inbox_uploads_tests.rs"]
mod tests;
