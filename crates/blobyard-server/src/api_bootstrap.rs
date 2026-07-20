use crate::{
    api::AppState,
    auth::{self, OPERATOR_SCOPES},
    error::ApiError,
    response::{Success, success},
    slug, transfer_grants,
};
use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use blobyard_contract::{LocalApiTokenRecord, LocalCliSessionRecord, RepositoryError};
use blobyard_core::{GeneratedSecretKind, SecretString};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BootstrapRequest {
    name: String,
    platform: String,
    token: SecretString,
    version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BootstrapResponse {
    access_token: SecretString,
    scopes: Vec<String>,
    web_yard_origin: String,
    workspace: String,
}

pub(super) async fn exchange_bootstrap(
    State(state): State<AppState>,
    payload: Result<Json<BootstrapRequest>, JsonRejection>,
) -> Result<Json<Success<BootstrapResponse>>, ApiError> {
    let Json(request) = ApiError::invalid_request_result(payload)?;
    slug::validate_name(&request.name)?;
    transfer_grants::validate_field(&request.platform)?;
    transfer_grants::validate_field(&request.version)?;
    let access_token = auth::generate_token(GeneratedSecretKind::AccessToken);
    exchange_bootstrap_at(&state, request, access_token, transfer_grants::now_ms())
}

fn exchange_bootstrap_at(
    state: &AppState,
    request: BootstrapRequest,
    access_token: SecretString,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<BootstrapResponse>>, ApiError> {
    let created_at_ms = now?;
    let scopes = OPERATOR_SCOPES
        .iter()
        .map(|scope| (*scope).to_owned())
        .collect::<Vec<_>>();
    let token_id = format!("token_{}", uuid::Uuid::new_v4().simple());
    let record = LocalApiTokenRecord {
        id: token_id.clone(),
        name: request.name.clone(),
        token_prefix: access_token.expose_secret().chars().take(16).collect(),
        secret_hash: auth::hash(access_token.expose_secret()),
        scopes: scopes.clone(),
        workspace_id: state.default_workspace.id.clone(),
        project_id: None,
        created_at_ms,
        expires_at_ms: i64::MAX as u64,
        last_used_at_ms: None,
        revoked_at_ms: None,
    };
    let session = LocalCliSessionRecord {
        id: format!("session_{}", uuid::Uuid::new_v4().simple()),
        token_id,
        workspace_id: state.default_workspace.id.clone(),
        name: request.name,
        platform: request.platform,
        version: request.version,
        created_at_ms,
        last_used_at_ms: None,
        revoked_at_ms: None,
    };
    state
        .repository
        .exchange_bootstrap(
            &auth::hash(request.token.expose_secret()),
            &record,
            &session,
        )
        .map_err(bootstrap_error)?;
    crate::audit::bootstrap_exchanged(state, &record)?;
    Ok(success(BootstrapResponse {
        access_token,
        scopes,
        web_yard_origin: state.web_yard_origin.clone(),
        workspace: state.default_workspace.slug.to_string(),
    }))
}

const fn bootstrap_error(error: RepositoryError) -> ApiError {
    match error {
        RepositoryError::NotFound | RepositoryError::InvalidInput => ApiError::invalid_token(),
        other => ApiError::from_repository(other),
    }
}

#[cfg(test)]
#[path = "api_bootstrap_tests.rs"]
mod tests;
