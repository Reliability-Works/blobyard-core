use crate::{
    api::AppState,
    auth::hash,
    error::ApiError,
    inbox_upload_auth::{AuthorizedUpload, RateKind, UploadAuthority},
    response::{Success, success},
    transfer_grants as grants,
};
use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State, rejection::JsonRejection};
use axum::http::{HeaderValue, StatusCode, header::ETAG};
use axum::response::{IntoResponse, Response};
use blobyard_api_client::{RequestUploadPartsRequest, RequestUploadPartsResponse, UploadPartGrant};
use blobyard_contract::{
    AuditValue, NewUploadPartGrant, ReservationStrategy, UploadReservationRecord,
};
use blobyard_core::SecretString;
use std::collections::HashSet;

/// Issues a bounded batch of retry-stable multipart part capabilities.
pub(crate) async fn request_parts(
    State(state): State<AppState>,
    authority: UploadAuthority,
    payload: Result<Json<RequestUploadPartsRequest>, JsonRejection>,
) -> Result<Json<Success<RequestUploadPartsResponse>>, ApiError> {
    request_parts_at(State(state), authority, payload, grants::now_ms())
}

fn request_parts_at(
    State(state): State<AppState>,
    authority: UploadAuthority,
    payload: Result<Json<RequestUploadPartsRequest>, JsonRejection>,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<RequestUploadPartsResponse>>, ApiError> {
    let now = now?;
    let authority = authority.authorize_at(&state, RateKind::Transfer, now)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    let reservation = match &authority {
        AuthorizedUpload::Operator(principal) => {
            crate::transfers::authorize_upload(&state, &principal.0, &request.upload_id)?;
            state
                .repository
                .upload_by_id(&request.upload_id)
                .map_err(ApiError::from_repository)?
        }
        AuthorizedUpload::Inbox(guest) => {
            crate::inbox_uploads::reservation(&state, guest, &request.upload_id, now)?
        }
    };
    let grants = build_grants(&state, &reservation, &request.part_numbers, now)?;
    state
        .repository
        .issue_upload_parts(
            &grants
                .iter()
                .map(|grant| grant.input.clone())
                .collect::<Vec<_>>(),
        )
        .map_err(ApiError::from_repository)?;
    if let AuthorizedUpload::Operator(principal) = authority {
        crate::audit::record_action(
            &state,
            &principal.0,
            "transfer.upload_parts_requested",
            "upload",
            vec![
                ("uploadId".to_owned(), AuditValue::String(request.upload_id)),
                (
                    "partCount".to_owned(),
                    AuditValue::Number(grants.len() as u64),
                ),
            ],
        )?;
    }
    parts_response(grants, reservation.expires_at_ms)
}

fn parts_response(
    grants: Vec<PartGrant>,
    expires_at_ms: u64,
) -> Result<Json<Success<RequestUploadPartsResponse>>, ApiError> {
    Ok(success(RequestUploadPartsResponse {
        parts: grants.into_iter().map(|grant| grant.response).collect(),
        expires_at: grants::format_expiry(expires_at_ms)?,
    }))
}

#[derive(Clone)]
struct PartGrant {
    input: NewUploadPartGrant,
    response: UploadPartGrant,
}

fn build_grants(
    state: &AppState,
    upload: &UploadReservationRecord,
    numbers: &[u32],
    now: u64,
) -> Result<Vec<PartGrant>, ApiError> {
    let count = upload.part_count.ok_or_else(ApiError::conflict)?;
    let size = upload.part_size.ok_or_else(ApiError::conflict)?;
    if upload.strategy != ReservationStrategy::Multipart
        || upload.provider_upload_id.is_none()
        || upload.expires_at_ms <= now
    {
        return Err(ApiError::conflict());
    }
    if numbers.is_empty() || numbers.len() > 100 {
        return Err(ApiError::invalid_request());
    }
    let mut unique = HashSet::with_capacity(numbers.len());
    numbers
        .iter()
        .map(|number| {
            if *number == 0 || *number > count || !unique.insert(*number) {
                return Err(ApiError::invalid_request());
            }
            let capability = grants::stable_part_capability(state, &upload.id, *number);
            Ok(PartGrant {
                input: NewUploadPartGrant {
                    upload_id: upload.id.clone(),
                    part_number: *number,
                    expected_size: part_size(upload.expected_size, size, *number),
                    capability_hash: hash(capability.expose_secret()),
                    expires_at_ms: upload.expires_at_ms,
                },
                response: UploadPartGrant {
                    part_number: *number,
                    upload_url: grants::transfer_url(
                        &state.public_origin,
                        "transfers/upload-parts",
                        &capability,
                    )?,
                },
            })
        })
        .collect()
}

fn part_size(total: u64, size: u64, number: u32) -> u64 {
    let offset = u64::from(number - 1).saturating_mul(size);
    total.saturating_sub(offset).min(size)
}

/// Receives one exact multipart part through its one-purpose capability.
pub(crate) async fn put_part(
    State(state): State<AppState>,
    Path(capability): Path<String>,
    body: Body,
) -> Result<Response, ApiError> {
    let capability = ApiError::not_found_result(SecretString::new(capability))?;
    put_part_at(&state, &capability, body, grants::now_ms()).await
}

async fn put_part_at(
    state: &AppState,
    capability: &SecretString,
    body: Body,
    now: Result<u64, ApiError>,
) -> Result<Response, ApiError> {
    let (upload, part) = state
        .repository
        .upload_part_by_capability(&hash(capability.expose_secret()), now?)
        .map_err(grants::conceal_capability_error)?;
    let metadata = crate::transfer_io::receive_part(state, &upload, &part, body).await?;
    state
        .repository
        .record_uploaded_part(
            &upload.id,
            part.part_number,
            metadata.size,
            metadata.checksum.as_str(),
            metadata.provider_tag.as_deref(),
        )
        .map_err(ApiError::from_repository)?;
    part_response(metadata.checksum.as_str())
}

fn part_response(checksum: &str) -> Result<Response, ApiError> {
    let etag = etag_header(checksum)?;
    Ok((StatusCode::NO_CONTENT, [(ETAG, etag)]).into_response())
}

fn etag_header(checksum: &str) -> Result<HeaderValue, ApiError> {
    HeaderValue::from_str(&format!("\"{checksum}\"")).map_err(|_error| ApiError::internal())
}

#[cfg(test)]
#[path = "transfer_multipart_http_tests.rs"]
mod tests;
