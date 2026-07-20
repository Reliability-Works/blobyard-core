use crate::{
    api::{self, AppState, CredentialRevokeRequest, RevokeTarget},
    auth::{self, OPERATOR_SCOPES, Principal},
    error::ApiError,
    response::{Success, success},
    transfer_grants,
};
use axum::{
    Json, Router,
    extract::{State, rejection::JsonRejection},
    routing::{get, post},
};
use blobyard_contract::LocalApiTokenRecord;
use blobyard_core::{GeneratedSecretKind, SecretString, Slug};
use serde::{Deserialize, Serialize};

const DAY_MS: u64 = 24 * 60 * 60 * 1_000;
const EXPIRY_DAYS: [u64; 3] = [7, 30, 90];

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/api-tokens", get(list).post(create))
        .route("/v1/api-tokens/revoke", post(api::revoke::<RevokeRequest>))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateRequest {
    expires_in_days: u64,
    name: String,
    project: Option<Slug>,
    scopes: Vec<String>,
    workspace: Option<Slug>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateResponse {
    expires_at: u64,
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    raw_token: SecretString,
    scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<CreateRequest>, JsonRejection>,
) -> Result<Json<Success<CreateResponse>>, ApiError> {
    let Json(request) = ApiError::invalid_request_result(payload)?;
    create_with_clock(&state, &principal, request, transfer_grants::now_ms())
}

fn create_with_clock(
    state: &AppState,
    principal: &Principal,
    request: CreateRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<CreateResponse>>, ApiError> {
    create_at(state, principal, request, now?)
}

fn create_at(
    state: &AppState,
    principal: &Principal,
    request: CreateRequest,
    now_ms: u64,
) -> Result<Json<Success<CreateResponse>>, ApiError> {
    principal.require("tokens:manage")?;
    let name = normalize_name(&request.name)?;
    let scopes = normalize_scopes(&request.scopes, &principal.0.scopes)?;
    let expires_at_ms = expiration(now_ms, request.expires_in_days)?;
    let binding = resolve_binding(
        state,
        principal,
        &scopes,
        request.workspace,
        request.project,
    )?;
    let raw_token = auth::generate_token(GeneratedSecretKind::ApiToken);
    let token = LocalApiTokenRecord {
        id: format!("token_{}", uuid::Uuid::new_v4().simple()),
        name,
        token_prefix: raw_token.expose_secret().chars().take(16).collect(),
        secret_hash: auth::hash(raw_token.expose_secret()),
        scopes,
        workspace_id: binding
            .as_ref()
            .map_or_else(|| principal.0.workspace_id.clone(), |value| value.0.clone()),
        project_id: binding.as_ref().map(|value| value.1.clone()),
        created_at_ms: now_ms,
        expires_at_ms,
        last_used_at_ms: None,
        revoked_at_ms: None,
    };
    let event = crate::audit::api_token_created_event(&principal.0, &token);
    state
        .repository
        .create_api_token(&token, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(CreateResponse {
        expires_at: token.expires_at_ms,
        id: token.id,
        name: token.name,
        project_id: token.project_id.clone(),
        raw_token,
        scopes: token.scopes,
        workspace_id: token.project_id.map(|_project| token.workspace_id),
    }))
}

fn normalize_name(value: &str) -> Result<String, ApiError> {
    let name = value.trim();
    if (2..=80).contains(&name.len()) && !name.chars().any(char::is_control) {
        Ok(name.to_owned())
    } else {
        Err(ApiError::invalid_request())
    }
}

fn normalize_scopes(requested: &[String], caller: &[String]) -> Result<Vec<String>, ApiError> {
    if requested.is_empty()
        || requested
            .iter()
            .any(|scope| !OPERATOR_SCOPES.contains(&scope.as_str()))
    {
        return Err(ApiError::invalid_request());
    }
    if requested.iter().any(|scope| !caller.contains(scope)) {
        return Err(ApiError::forbidden());
    }
    Ok(OPERATOR_SCOPES
        .iter()
        .filter(|scope| requested.iter().any(|requested| requested == *scope))
        .map(|scope| (*scope).to_owned())
        .collect())
}

fn expiration(now_ms: u64, days: u64) -> Result<u64, ApiError> {
    if !EXPIRY_DAYS.contains(&days) {
        return Err(ApiError::invalid_request());
    }
    days.checked_mul(DAY_MS)
        .and_then(|duration| now_ms.checked_add(duration))
        .ok_or_else(ApiError::invalid_request)
}

fn resolve_binding(
    state: &AppState,
    principal: &Principal,
    scopes: &[String],
    workspace: Option<Slug>,
    project: Option<Slug>,
) -> Result<Option<(String, String)>, ApiError> {
    let cleanup = scopes == ["object:write"];
    if !cleanup {
        return if workspace.is_none() && project.is_none() {
            Ok(None)
        } else {
            Err(ApiError::invalid_request())
        };
    }
    let workspace = workspace.ok_or_else(ApiError::invalid_request)?;
    let project = project.ok_or_else(ApiError::invalid_request)?;
    let workspace = state
        .repository
        .workspace_by_slug(&workspace)
        .map_err(ApiError::from_repository)?;
    if workspace.id != principal.0.workspace_id {
        return Err(ApiError::not_found());
    }
    let project = state
        .repository
        .project_by_slug(&workspace.id, &project)
        .map_err(ApiError::from_repository)?;
    Ok(Some((workspace.id, project.id)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenSummary {
    created_at: u64,
    expires_at: u64,
    id: String,
    last_used_at: Option<u64>,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    scopes: Vec<String>,
    status: &'static str,
    token_prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<Success<Vec<TokenSummary>>>, ApiError> {
    list_with_clock(&state, &principal, transfer_grants::now_ms())
}

fn list_with_clock(
    state: &AppState,
    principal: &Principal,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<Vec<TokenSummary>>>, ApiError> {
    principal.require("tokens:manage")?;
    let now_ms = now?;
    let tokens = state
        .repository
        .list_api_tokens()
        .map_err(ApiError::from_repository)?
        .into_iter()
        .map(|token| TokenSummary::from_record(token, now_ms))
        .collect();
    Ok(success(tokens))
}

impl TokenSummary {
    fn from_record(token: LocalApiTokenRecord, now_ms: u64) -> Self {
        let status = if token.revoked_at_ms.is_some() {
            "revoked"
        } else if token.expires_at_ms <= now_ms {
            "expired"
        } else {
            "active"
        };
        let bound = token.project_id.is_some();
        Self {
            created_at: token.created_at_ms,
            expires_at: token.expires_at_ms,
            id: token.id,
            last_used_at: token.last_used_at_ms,
            name: token.name,
            project_id: token.project_id,
            scopes: token.scopes,
            status,
            token_prefix: token.token_prefix,
            workspace_id: bound.then_some(token.workspace_id),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevokeRequest {
    token_id: String,
}

impl CredentialRevokeRequest for RevokeRequest {
    const REQUIRED_SCOPE: &'static str = "tokens:manage";

    fn into_target(self) -> RevokeTarget {
        RevokeTarget::ApiToken(self.token_id)
    }
}

#[cfg(test)]
#[path = "api_tokens_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "api_tokens_http_tests.rs"]
mod http_tests;
