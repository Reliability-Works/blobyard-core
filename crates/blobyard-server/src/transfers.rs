use crate::api::AppState;
use crate::auth::hash;
use crate::error::ApiError;
use crate::inbox_upload_auth::{AuthorizedUpload, RateKind, UploadAuthority};
use crate::response::Success;
use crate::transfer_grants as grants;
#[cfg(any(test, feature = "test-seams"))]
pub(super) use crate::transfers_operations::issue_upload_at;
pub(super) use crate::transfers_operations::upload_response;
pub(crate) use crate::transfers_operations::{authorize_upload, status_response};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    routing::{get, post, put},
};
use blobyard_api_client::{
    AbortUploadRequest, CompleteUploadRequest, CompleteUploadResponse, EmptyResponse,
    RequestUploadRequest, RequestUploadResponse, UploadStatusQuery, UploadStatusResponse,
};
use blobyard_core::SecretString;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/uploads/request", post(request_upload))
        .route(
            "/v1/uploads/parts/request",
            post(crate::transfer_multipart_http::request_parts),
        )
        .route("/v1/uploads/complete", post(complete_upload))
        .route("/v1/uploads/abort", post(abort_upload))
        .route("/v1/uploads/status", get(upload_status))
        .route("/transfers/uploads/{capability}", put(put_upload))
        .route(
            "/transfers/upload-parts/{capability}",
            put(crate::transfer_multipart_http::put_part),
        )
}

async fn request_upload(
    State(state): State<AppState>,
    authority: UploadAuthority,
    headers: HeaderMap,
    payload: Result<Json<RequestUploadRequest>, JsonRejection>,
) -> Result<Json<Success<RequestUploadResponse>>, ApiError> {
    request_upload_at(&state, authority, &headers, payload, grants::now_ms())
}

fn request_upload_at(
    state: &AppState,
    authority: UploadAuthority,
    headers: &HeaderMap,
    payload: Result<Json<RequestUploadRequest>, JsonRejection>,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<RequestUploadResponse>>, ApiError> {
    let now = now?;
    let authority = authority.authorize_at(state, RateKind::Upload, now)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    let idempotency = grants::idempotency_key(headers)?;
    match authority {
        AuthorizedUpload::Operator(principal) => {
            crate::transfers_operations::request_operator_upload(
                state,
                &principal,
                &request,
                idempotency,
                now,
            )
        }
        AuthorizedUpload::Inbox(guest) => {
            crate::inbox_uploads::issue_at(state, &guest, &request, idempotency, now)
        }
    }
}

async fn put_upload(
    State(state): State<AppState>,
    Path(capability): Path<String>,
    body: Body,
) -> Result<StatusCode, ApiError> {
    let capability = ApiError::not_found_result(SecretString::new(capability))?;
    put_upload_at(&state, &capability, body, grants::now_ms()).await
}

pub(super) async fn put_upload_at(
    state: &AppState,
    capability: &SecretString,
    body: Body,
    now: Result<u64, ApiError>,
) -> Result<StatusCode, ApiError> {
    let now = now?;
    let reservation = state
        .repository
        .upload_by_capability(&hash(capability.expose_secret()), now)
        .map_err(grants::conceal_capability_error)?;
    let metadata = crate::transfer_io::receive(state, &reservation, body).await?;
    state
        .repository
        .record_uploaded_bytes(&reservation.id, metadata.size, metadata.checksum.as_str())
        .map_err(ApiError::from_repository)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn complete_upload(
    State(state): State<AppState>,
    authority: UploadAuthority,
    payload: Result<Json<CompleteUploadRequest>, JsonRejection>,
) -> Result<Json<Success<CompleteUploadResponse>>, ApiError> {
    let (now, authority, request) = transfer_request(&state, authority, payload, grants::now_ms())?;
    match authority {
        AuthorizedUpload::Operator(principal) => {
            crate::transfers_operations::complete_operator_upload(&state, &principal, request)
        }
        AuthorizedUpload::Inbox(guest) => {
            crate::inbox_uploads::complete_at(&state, &guest, &request, now)
        }
    }
}

async fn abort_upload(
    State(state): State<AppState>,
    authority: UploadAuthority,
    payload: Result<Json<AbortUploadRequest>, JsonRejection>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let (now, authority, request) = transfer_request(&state, authority, payload, grants::now_ms())?;
    match authority {
        AuthorizedUpload::Operator(principal) => {
            crate::transfers_operations::abort_operator_upload(&state, &principal, request)
        }
        AuthorizedUpload::Inbox(guest) => {
            crate::inbox_uploads::abort_at(&state, &guest, &request, now)
        }
    }
}

fn transfer_request<T>(
    state: &AppState,
    authority: UploadAuthority,
    payload: Result<Json<T>, JsonRejection>,
    now: Result<u64, ApiError>,
) -> Result<(u64, AuthorizedUpload, T), ApiError> {
    let now = now?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    Ok((
        now,
        authority.authorize_at(state, RateKind::Transfer, now)?,
        request,
    ))
}

async fn upload_status(
    State(state): State<AppState>,
    authority: UploadAuthority,
    Query(query): Query<UploadStatusQuery>,
) -> Result<Json<Success<UploadStatusResponse>>, ApiError> {
    upload_status_at(&state, authority, &query, grants::now_ms())
}

fn upload_status_at(
    state: &AppState,
    authority: UploadAuthority,
    query: &UploadStatusQuery,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<UploadStatusResponse>>, ApiError> {
    let now = now?;
    match authority.authorize_at(state, RateKind::Transfer, now)? {
        AuthorizedUpload::Operator(principal) => {
            let reservation = state
                .repository
                .upload_by_id(&query.upload_id)
                .map_err(ApiError::from_repository)?;
            let project =
                grants::authorize_reservation(state, &principal.0.workspace_id, &reservation)?;
            grants::authorize_project_binding(&principal.0, &project)?;
            crate::transfers_operations::status_response(state, &reservation, now)
        }
        AuthorizedUpload::Inbox(guest) => {
            crate::inbox_uploads::status_at(state, &guest, &query.upload_id, now)
        }
    }
}

#[cfg(test)]
#[path = "transfers_tests.rs"]
mod tests;

/// Test-only transfer router fixtures for duplicated library contract coverage.
#[cfg(any(test, feature = "test-seams"))]
#[doc(hidden)]
#[path = "transfers_seams.rs"]
pub mod test_seams;

#[cfg(test)]
#[path = "transfers_contract_tests.rs"]
mod contract_tests;
