use crate::api::AppState;
use crate::auth::{Principal, generate_token, hash};
use crate::error::ApiError;
use crate::response::{Page, Success, page, success, success_with_request};
use crate::transfer_grants as grants;
use axum::{
    Json, Router,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::HeaderMap,
    response::Response,
    routing::{get, post},
};
use blobyard_api_client::{
    DeleteObjectRequest, DeleteObjectResponse, DownloadResponse, ListObjectsQuery,
    ObjectAvailability, ObjectSummary, RequestDownloadRequest,
};
use blobyard_contract::{
    AuditValue, CiAction, NewAuditEvent, NewDownloadGrant, NewObjectDeletion, ObjectDeletionTarget,
    StoredObjectRecord,
};
use blobyard_core::{BlobyardUri, GeneratedSecretKind, SecretString};
use std::num::NonZeroU64;

#[path = "objects_validation.rs"]
mod validation;
use validation::{object_source, validate_prefix};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/objects", get(list_objects).delete(delete_object))
        .route("/v1/downloads/request", post(request_download))
        .route("/transfers/downloads/{capability}", get(download))
}

async fn delete_object(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<DeleteObjectRequest>, JsonRejection>,
) -> Result<Json<Success<DeleteObjectResponse>>, ApiError> {
    principal.require("object:write")?;
    let Json(request) = payload.map_err(|_error| ApiError::invalid_request())?;
    delete_object_at(&state, principal, request, grants::now_ms())
}

fn delete_object_at(
    state: &AppState,
    principal: Principal,
    request: DeleteObjectRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<DeleteObjectResponse>>, ApiError> {
    let project = grants::resolve_project_slugs(
        state,
        &principal.0.workspace_id,
        request.uri.workspace_slug(),
        request.uri.project_slug(),
    )?;
    grants::authorize_project_binding(&principal.0, &project)?;
    let request_id = crate::error::request_id();
    let now = now?;
    let operation = NewObjectDeletion {
        id: format!("delete_{}", uuid::Uuid::new_v4().simple()),
        target: ObjectDeletionTarget {
            project_id: project.id,
            object_path: request.uri.logical_path().to_owned(),
            version: request.uri.version().map(NonZeroU64::get),
        },
        actor: principal.0.id.clone(),
        request_id: request_id.clone(),
        created_at_ms: now,
    };
    let plan = state
        .repository
        .begin_object_deletion(&operation)
        .map_err(ApiError::from_repository)?;
    let event = NewAuditEvent {
        id: format!("audit_{}", uuid::Uuid::new_v4().simple()),
        workspace_id: principal.0.workspace_id,
        actor: plan.actor.clone(),
        action: "object.deleted".to_owned(),
        request_id: plan.request_id.clone(),
        target_type: "object".to_owned(),
        metadata: vec![
            (
                "path".to_owned(),
                AuditValue::String(operation.target.object_path),
            ),
            (
                "version".to_owned(),
                operation
                    .target
                    .version
                    .map_or(AuditValue::Null, AuditValue::Number),
            ),
        ],
        created_at_ms: now,
    };
    crate::lifecycle::execute_deletion(state, &plan, now, &event)?;
    Ok(success_with_request(
        DeleteObjectResponse {
            uri: request.uri,
            deleted: true,
        },
        request_id,
    ))
}

async fn list_objects(
    State(state): State<AppState>,
    Principal(principal): Principal,
    Query(query): Query<ListObjectsQuery>,
) -> Result<Json<Success<Page<ObjectSummary>>>, ApiError> {
    let principal = Principal(principal);
    principal.require_action(CiAction::Download, "object:read")?;
    if query.cursor.is_some() {
        return Err(ApiError::invalid_request());
    }
    validate_prefix(query.prefix.as_deref())?;
    let project = grants::resolve_project_slugs(
        &state,
        &principal.0.workspace_id,
        &query.workspace,
        &query.project,
    )?;
    grants::authorize_project_binding(&principal.0, &project)?;
    let records =
        state
            .repository
            .list_stored_objects(&project.id, query.prefix.as_deref(), query.versions);
    object_page(&query, records)
}

fn object_page(
    query: &ListObjectsQuery,
    records: Result<Vec<StoredObjectRecord>, blobyard_contract::RepositoryError>,
) -> Result<Json<Success<Page<ObjectSummary>>>, ApiError> {
    let items = records
        .map_err(ApiError::from_repository)?
        .into_iter()
        .map(|record| object_summary(&query.workspace, &query.project, record))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(success(page(items)))
}

async fn request_download(
    State(state): State<AppState>,
    Principal(principal): Principal,
    payload: Result<Json<RequestDownloadRequest>, JsonRejection>,
) -> Result<Json<Success<DownloadResponse>>, ApiError> {
    let principal = Principal(principal);
    principal.require_action(CiAction::Download, "object:read")?;
    let Json(request) = payload.map_err(|_error| ApiError::invalid_request())?;
    let object = resolve_object(&state, &principal.0.workspace_id, &request.uri)?;
    let capability = generate_token(GeneratedSecretKind::DownloadCapability);
    request_download_at(&state, &principal.0, &object, &capability, grants::now_ms())
}

fn request_download_at(
    state: &AppState,
    principal: &blobyard_contract::LocalApiTokenRecord,
    object: &StoredObjectRecord,
    capability: &SecretString,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<DownloadResponse>>, ApiError> {
    let expires_at_ms = now?
        .checked_add(grants::GRANT_LIFETIME_MS)
        .ok_or_else(ApiError::internal)?;
    let response = download_response(state, object, capability, expires_at_ms)?;
    state
        .repository
        .issue_download(&NewDownloadGrant {
            version_id: object.version.id.clone(),
            capability_hash: hash(capability.expose_secret()),
            expires_at_ms,
        })
        .map_err(ApiError::from_repository)?;
    crate::audit::record_action(
        state,
        principal,
        "transfer.download_requested",
        "object",
        vec![(
            "versionId".to_owned(),
            AuditValue::String(object.version.id.clone()),
        )],
    )?;
    Ok(success(response))
}

pub(crate) fn download_response(
    state: &AppState,
    object: &StoredObjectRecord,
    capability: &SecretString,
    expires_at_ms: u64,
) -> Result<DownloadResponse, ApiError> {
    Ok(DownloadResponse {
        download_url: grants::transfer_url(
            &state.public_origin,
            "transfers/downloads",
            capability,
        )?,
        filename: object.filename.clone(),
        size_bytes: object.version.size.ok_or_else(ApiError::internal)?,
        checksum_sha256: object
            .version
            .checksum
            .clone()
            .ok_or_else(ApiError::internal)?,
        expires_at: grants::format_expiry(expires_at_ms)?,
    })
}

async fn download(
    State(state): State<AppState>,
    Path(capability): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    download_at(
        &state,
        ApiError::not_found_result(SecretString::new(capability)),
        &headers,
        grants::now_ms(),
    )
    .await
}

async fn download_at(
    state: &AppState,
    capability: Result<SecretString, ApiError>,
    headers: &HeaderMap,
    now: Result<u64, ApiError>,
) -> Result<Response, ApiError> {
    let capability = capability?;
    let object = state
        .repository
        .download_by_capability(&hash(capability.expose_secret()), now?)
        .map_err(grants::conceal_capability_error)?;
    crate::download_io::response(state, &object, headers).await
}

pub(crate) fn resolve_object(
    state: &AppState,
    workspace_id: &str,
    uri: &BlobyardUri,
) -> Result<StoredObjectRecord, ApiError> {
    let project = grants::resolve_project_slugs(
        state,
        workspace_id,
        uri.workspace_slug(),
        uri.project_slug(),
    )?;
    state
        .repository
        .list_stored_objects(&project.id, Some(uri.logical_path()), true)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .filter(|record| record.version.object_path == uri.logical_path())
        .filter(|record| {
            uri.version()
                .is_none_or(|version| version.get() == record.version.version)
        })
        .max_by_key(|record| record.version.version)
        .ok_or_else(ApiError::not_found)
}

fn object_summary(
    workspace: &blobyard_core::Slug,
    project: &blobyard_core::Slug,
    record: StoredObjectRecord,
) -> Result<ObjectSummary, ApiError> {
    let version = NonZeroU64::new(record.version.version).ok_or_else(ApiError::internal)?;
    Ok(ObjectSummary {
        uri: BlobyardUri::new(
            workspace.clone(),
            project.clone(),
            record.version.object_path,
            Some(version),
        )
        .map_err(|_error| ApiError::internal())?,
        filename: record.filename,
        size_bytes: record.version.size.ok_or_else(ApiError::internal)?,
        created_at: grants::format_expiry(record.version.created_at_ms)?,
        availability: ObjectAvailability::Available,
        source: object_source(record.version.source),
    })
}

#[cfg(test)]
#[path = "objects_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "objects_contract_tests.rs"]
mod contract_tests;

#[cfg(any(test, feature = "test-seams"))]
#[path = "objects_seams.rs"]
pub mod test_seams;
