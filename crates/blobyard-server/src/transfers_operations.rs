use crate::{
    api::AppState,
    auth::Principal,
    error::ApiError,
    response::{Success, success},
    transfer_grants as grants,
};
use axum::Json;
use blobyard_api_client::{
    AbortUploadRequest, CompleteUploadRequest, CompleteUploadResponse, EmptyResponse,
    RequestUploadRequest, RequestUploadResponse, UploadStatusResponse, UploadStrategy,
};
use blobyard_contract::{
    AuditValue, LocalApiTokenRecord, ObjectVersionRecord, ProjectRecord, ReservationState,
    ReservationStrategy, UploadReservationRecord,
};
use blobyard_core::{BlobyardUri, SecretString, Slug};
use std::num::NonZeroU64;

pub(crate) fn request_operator_upload(
    state: &AppState,
    principal: &Principal,
    request: &RequestUploadRequest,
    idempotency: &str,
    now: u64,
) -> Result<Json<Success<RequestUploadResponse>>, ApiError> {
    grants::validate_field(&request.filename)?;
    grants::validate_field(&request.content_type)?;
    let project = grants::resolve_project(state, &principal.0.workspace_id, request)?;
    grants::authorize_project_binding(&principal.0, &project)?;
    ApiError::invalid_request_result(BlobyardUri::new(
        request.workspace.clone(),
        request.project.clone(),
        request.path.clone(),
        None,
    ))?;
    issue_upload_at(
        state,
        &principal.0,
        request,
        &project,
        idempotency,
        principal.object_source(),
        Ok(now),
    )
}

pub(crate) fn issue_upload_at(
    state: &AppState,
    principal: &LocalApiTokenRecord,
    request: &RequestUploadRequest,
    project: &ProjectRecord,
    idempotency: &str,
    source: blobyard_contract::ObjectSource,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<RequestUploadResponse>>, ApiError> {
    let now = now?;
    let expires_at_ms = now
        .checked_add(grants::GRANT_LIFETIME_MS)
        .ok_or_else(ApiError::internal)?;
    let upload_id = grants::stable_upload_id(&principal.id, idempotency);
    let capability = grants::stable_capability(state, &principal.id, idempotency);
    let upload_url = grants::transfer_url(&state.public_origin, "transfers/uploads", &capability)?;
    let input = grants::reservation_input(
        request,
        project,
        &upload_id,
        &capability,
        expires_at_ms,
        source,
    );
    grants::validate_upload_strategy(&input)?;
    let reservation = grants::reserve_or_replay(state, &input, now, expires_at_ms)?;
    let reservation = crate::transfer_multipart::ensure_provider(state, reservation)?;
    crate::audit::record_action(
        state,
        principal,
        "transfer.upload_requested",
        "upload",
        vec![("uploadId".to_owned(), AuditValue::String(upload_id.clone()))],
    )?;
    upload_response(
        upload_id,
        upload_url,
        reservation.strategy,
        reservation.part_size,
        reservation.expires_at_ms,
    )
}

pub(crate) fn upload_response(
    upload_id: String,
    upload_url: SecretString,
    strategy: ReservationStrategy,
    part_size: Option<u64>,
    expires_at_ms: u64,
) -> Result<Json<Success<RequestUploadResponse>>, ApiError> {
    let multipart = strategy == ReservationStrategy::Multipart;
    Ok(success(RequestUploadResponse {
        upload_id,
        strategy: if multipart {
            UploadStrategy::Multipart
        } else {
            UploadStrategy::Single
        },
        upload_url: (!multipart).then_some(upload_url),
        headers: Vec::new(),
        part_size_bytes: part_size,
        expires_at: grants::format_expiry(expires_at_ms)?,
    }))
}

pub(crate) fn complete_operator_upload(
    state: &AppState,
    principal: &Principal,
    request: CompleteUploadRequest,
) -> Result<Json<Success<CompleteUploadResponse>>, ApiError> {
    let project = authorize_upload(state, &principal.0, &request.upload_id)?;
    let reservation = state
        .repository
        .upload_by_id(&request.upload_id)
        .map_err(ApiError::from_repository)?;
    crate::transfer_multipart::complete(state, &reservation, &request.parts)?;
    let workspace = grants::workspace_by_id(state, &principal.0.workspace_id)?;
    let version = state
        .repository
        .complete_upload(&request.upload_id)
        .map_err(ApiError::from_repository)?;
    crate::audit::record_action(
        state,
        &principal.0,
        "transfer.upload_completed",
        "upload",
        vec![("uploadId".to_owned(), AuditValue::String(request.upload_id))],
    )?;
    completion_response(workspace.slug, project.slug, version)
}

pub(crate) fn completion_response(
    workspace: Slug,
    project: Slug,
    version: ObjectVersionRecord,
) -> Result<Json<Success<CompleteUploadResponse>>, ApiError> {
    let number = NonZeroU64::new(version.version).ok_or_else(ApiError::internal)?;
    let uri = ApiError::internal_result(BlobyardUri::new(
        workspace,
        project,
        version.object_path,
        Some(number),
    ))?;
    Ok(success(CompleteUploadResponse {
        uri,
        size_bytes: version.size.ok_or_else(ApiError::internal)?,
        checksum_sha256: version.checksum.ok_or_else(ApiError::internal)?,
    }))
}

pub(crate) fn abort_operator_upload(
    state: &AppState,
    principal: &Principal,
    request: AbortUploadRequest,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    authorize_upload(state, &principal.0, &request.upload_id)?;
    let reservation = state
        .repository
        .upload_by_id(&request.upload_id)
        .map_err(ApiError::from_repository)?;
    crate::transfer_multipart::abort_storage(state, &reservation)?;
    state
        .repository
        .abort_upload(&request.upload_id)
        .map_err(ApiError::from_repository)?;
    crate::audit::record_action(
        state,
        &principal.0,
        "transfer.upload_aborted",
        "upload",
        vec![("uploadId".to_owned(), AuditValue::String(request.upload_id))],
    )?;
    Ok(success(EmptyResponse::default()))
}

pub(crate) fn authorize_upload(
    state: &AppState,
    principal: &LocalApiTokenRecord,
    upload_id: &str,
) -> Result<ProjectRecord, ApiError> {
    let reservation = state
        .repository
        .upload_by_id(upload_id)
        .map_err(ApiError::from_repository)?;
    let project = grants::authorize_reservation(state, &principal.workspace_id, &reservation)?;
    grants::authorize_project_binding(principal, &project)?;
    Ok(project)
}

pub(crate) fn status_response(
    state: &AppState,
    reservation: &UploadReservationRecord,
    now: u64,
) -> Result<Json<Success<UploadStatusResponse>>, ApiError> {
    let status = match reservation.state {
        ReservationState::Requested if reservation.expires_at_ms <= now => "expired",
        ReservationState::Requested => "requested",
        ReservationState::Uploaded => "uploading",
        ReservationState::Complete => "complete",
        ReservationState::Aborted => "aborted",
    };
    let completed_parts = crate::transfer_multipart::completed_part_numbers(state, reservation)?;
    Ok(success(UploadStatusResponse {
        state: status.to_owned(),
        completed_parts,
    }))
}
