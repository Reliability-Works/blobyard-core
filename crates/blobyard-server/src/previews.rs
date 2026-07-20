use crate::{api::AppState, auth::hash, error::ApiError, response::Page};
use axum::{
    Router,
    body::Body,
    extract::{OriginalUri, State},
    http::{HeaderMap, Method, Response},
    routing::{get, post},
};
use blobyard_api_client::{
    CreatePreviewRequest, CreatePreviewResponse, EmptyResponse, ListPreviewsQuery, PreviewSummary,
    RevokePreviewRequest,
};
use blobyard_contract::CiAction;

#[path = "previews_contracts.rs"]
mod contracts;
#[path = "previews_operations.rs"]
mod operations;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/previews", get(list_previews).post(create_preview))
        .route("/v1/previews/revoke", post(revoke_preview))
}

crate::auth::managed_json_handler!(
    create_preview,
    CreatePreviewRequest,
    CreatePreviewResponse,
    CiAction::Share,
    "share:manage",
    operations::create_at
);
crate::auth::managed_query_handler!(
    list_previews,
    ListPreviewsQuery,
    Page<PreviewSummary>,
    CiAction::Share,
    "share:manage",
    operations::list_at
);
crate::auth::managed_json_handler!(
    revoke_preview,
    RevokePreviewRequest,
    EmptyResponse,
    CiAction::Share,
    "share:manage",
    operations::revoke_at
);

pub(crate) async fn public_fallback(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
) -> Result<Response<Body>, ApiError> {
    public_fallback_at(
        &state,
        &uri,
        &method,
        &headers,
        crate::transfer_grants::now_ms(),
    )
    .await
}

async fn public_fallback_at(
    state: &AppState,
    uri: &axum::http::Uri,
    method: &Method,
    headers: &HeaderMap,
    now: Result<u64, ApiError>,
) -> Result<Response<Body>, ApiError> {
    if method != Method::GET && method != Method::HEAD {
        return Err(ApiError::not_found());
    }
    let authority = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(ApiError::not_found)?;
    let capability = contracts::public_host_capability(&state.web_yard_origin, authority)
        .ok_or_else(ApiError::not_found)?;
    let path = contracts::public_preview_path(uri.path())?;
    let target = state
        .repository
        .preview_file_by_capability(&hash(&capability), &path, now?)
        .map_err(ApiError::concealed_capability)?;
    crate::download_io::public_site_response(state, &target.object, headers, method).await
}

#[cfg(test)]
#[path = "previews_tests.rs"]
pub mod tests;
