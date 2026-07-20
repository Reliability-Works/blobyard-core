use crate::{
    api::AppState,
    error::ApiError,
    response::{Page, Success, success},
};
use axum::{
    Json, Router,
    body::Body,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::Response,
    routing::{get, post},
};
use blobyard_api_client::{
    CreateShareRequest, CreateShareResponse, DownloadResponse, EmptyResponse, ListSharesQuery,
    ResolveShareQuery, RevokeShareRequest, ShareDownloadRequest, ShareMetadata, ShareSummary,
};
use blobyard_contract::CiAction;
use blobyard_core::SecretString;

#[path = "shares_contracts.rs"]
mod contracts;

#[path = "shares_operations.rs"]
mod operations;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/shares", get(list_shares).post(create_share))
        .route("/v1/shares/resolve", get(resolve_share))
        .route("/v1/shares/download", post(download_share))
        .route("/v1/shares/revoke", post(revoke_share))
        .route("/s/{token}", get(open_share))
        .route("/s/{token}/download", post(download_shared_file))
}

crate::auth::managed_json_handler!(
    create_share,
    CreateShareRequest,
    CreateShareResponse,
    CiAction::Share,
    "share:manage",
    operations::create_at
);
crate::auth::managed_query_handler!(
    list_shares,
    ListSharesQuery,
    Page<ShareSummary>,
    CiAction::Share,
    "share:manage",
    operations::list_at
);

async fn resolve_share(
    State(state): State<AppState>,
    query: Result<Query<ResolveShareQuery>, QueryRejection>,
) -> Result<Json<Success<ShareMetadata>>, ApiError> {
    let Query(query) = ApiError::not_found_result(query)?;
    operations::resolve_at(&state, &query, crate::transfer_grants::now_ms())
}

async fn download_share(
    State(state): State<AppState>,
    payload: Result<Json<ShareDownloadRequest>, JsonRejection>,
) -> Result<Json<Success<DownloadResponse>>, ApiError> {
    let Json(request) = ApiError::not_found_result(payload)?;
    Ok(success(operations::issue_share_download_at(
        &state,
        &request.token,
        crate::transfer_grants::now_ms(),
    )?))
}

async fn open_share(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Response<Body>, ApiError> {
    operations::open_at(
        &state,
        ApiError::not_found_result(SecretString::new(token)),
        crate::transfer_grants::now_ms(),
    )
}

async fn download_shared_file(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Response<Body>, ApiError> {
    operations::download_shared_file_at(
        &state,
        ApiError::not_found_result(SecretString::new(token)),
        crate::transfer_grants::now_ms(),
    )
}

crate::auth::managed_json_handler!(
    revoke_share,
    RevokeShareRequest,
    EmptyResponse,
    CiAction::Share,
    "share:manage",
    operations::revoke_at
);

#[cfg(test)]
#[path = "shares_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "shares_contract_tests.rs"]
mod contract_tests;

#[cfg(test)]
#[path = "shares_failure_tests.rs"]
mod failure_tests;

#[cfg(test)]
#[path = "shares_operation_tests.rs"]
mod operation_tests;
