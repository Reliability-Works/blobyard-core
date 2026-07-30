use crate::api::AppState;
use crate::auth::{self, Principal};
use crate::error::ApiError;
use crate::response::{Success, success};
use crate::transfer_grants;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use blobyard_api_client::{
    CreateLocalUserRequest, CreateLocalUserResponse, DeactivateLocalUserRequest, EmptyResponse,
    ListLocalUsersQuery, ListLocalUsersResponse, LocalUserSummary, ResetLocalUserLoginKeyRequest,
    ResetLocalUserLoginKeyResponse,
};
use blobyard_contract::{
    LocalUserListing, LocalUserLoginKeyRecord, LocalUserRecord, LocalUserStatus, WorkspaceRecord,
};
use blobyard_core::{GeneratedSecretKind, SecretString};

/// The `api_tokens` far-future sentinel: key expiry policy is an operator concern.
const FAR_FUTURE_EXPIRY_MS: u64 = 9_223_372_036_854_775_807;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/users", get(list).post(create))
        .route("/v1/users/reset-key", post(reset_key))
        .route("/v1/users/deactivate", post(deactivate))
}

async fn create(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<CreateLocalUserRequest>, JsonRejection>,
) -> Result<Json<Success<CreateLocalUserResponse>>, ApiError> {
    require_users_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    create_with_clock(&state, &principal, &request, transfer_grants::now_ms())
}

fn create_with_clock(
    state: &AppState,
    principal: &Principal,
    request: &CreateLocalUserRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<CreateLocalUserResponse>>, ApiError> {
    let now_ms = now?;
    let workspace = authorized_workspace(state, principal, &request.workspace)?;
    let display_name = crate::api_tokens::normalize_name(&request.display_name)?;
    let email = request.email.as_deref().map(normalize_email).transpose()?;
    let raw_key = auth::generate_token(GeneratedSecretKind::UserLoginKey);
    let user = LocalUserRecord {
        id: format!("user_{}", uuid::Uuid::new_v4().simple()),
        workspace_id: workspace.id,
        display_name,
        email,
        status: LocalUserStatus::Active,
        created_at_ms: now_ms,
        deactivated_at_ms: None,
    };
    let key = new_login_key(&user.id, &raw_key, now_ms);
    let event = crate::audit::local_user_event(&principal.0, "user.created", &user.id, now_ms);
    let user_summary = summary(LocalUserListing {
        user: user.clone(),
        active_key_prefix: Some(key.token_prefix.clone()),
    })?;
    state
        .repository
        .create_local_user(&user, &key, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(CreateLocalUserResponse {
        login_key: raw_key,
        login_key_prefix: key.token_prefix,
        user: user_summary,
    }))
}

async fn list(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<ListLocalUsersQuery>, QueryRejection>,
) -> Result<Json<Success<ListLocalUsersResponse>>, ApiError> {
    require_users_manage(&principal)?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    let workspace = authorized_workspace(&state, &principal, &query.workspace)?;
    let users = state
        .repository
        .list_local_users(&workspace.id)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .map(summary)
        .collect::<Result<Vec<_>, _>>();
    users.map(|users| success(ListLocalUsersResponse { users }))
}

async fn reset_key(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<ResetLocalUserLoginKeyRequest>, JsonRejection>,
) -> Result<Json<Success<ResetLocalUserLoginKeyResponse>>, ApiError> {
    require_users_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    reset_with_clock(&state, &principal, &request, transfer_grants::now_ms())
}

fn reset_with_clock(
    state: &AppState,
    principal: &Principal,
    request: &ResetLocalUserLoginKeyRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<ResetLocalUserLoginKeyResponse>>, ApiError> {
    let now_ms = now?;
    let user = workspace_user(state, principal, &request.user_id)?;
    let raw_key = auth::generate_token(GeneratedSecretKind::UserLoginKey);
    let key = new_login_key(&user.id, &raw_key, now_ms);
    let event =
        crate::audit::local_user_event(&principal.0, "user.login_key_reset", &user.id, now_ms);
    state
        .repository
        .reset_local_user_login_key(&key, now_ms, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(ResetLocalUserLoginKeyResponse {
        login_key: raw_key,
        login_key_prefix: key.token_prefix,
    }))
}

async fn deactivate(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<DeactivateLocalUserRequest>, JsonRejection>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    require_users_manage(&principal)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    deactivate_with_clock(&state, &principal, &request, transfer_grants::now_ms())
}

fn deactivate_with_clock(
    state: &AppState,
    principal: &Principal,
    request: &DeactivateLocalUserRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let now_ms = now?;
    let user = workspace_user(state, principal, &request.user_id)?;
    let event = crate::audit::local_user_event(&principal.0, "user.deactivated", &user.id, now_ms);
    state
        .repository
        .deactivate_local_user(&user.id, now_ms, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(EmptyResponse::default()))
}

pub(crate) fn require_users_manage(principal: &Principal) -> Result<(), ApiError> {
    if principal.is_machine() {
        Err(ApiError::forbidden())
    } else {
        principal.require("users:manage")
    }
}

pub(crate) fn authorized_workspace(
    state: &AppState,
    principal: &Principal,
    workspace: &blobyard_core::Slug,
) -> Result<WorkspaceRecord, ApiError> {
    let workspace = state
        .repository
        .workspace_by_slug(workspace)
        .map_err(ApiError::from_repository)?;
    if workspace.id == principal.0.workspace_id {
        Ok(workspace)
    } else {
        Err(ApiError::not_found())
    }
}

fn workspace_user(
    state: &AppState,
    principal: &Principal,
    user_id: &str,
) -> Result<LocalUserRecord, ApiError> {
    state
        .repository
        .list_local_users(&principal.0.workspace_id)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .map(|listing| listing.user)
        .find(|user| user.id == user_id)
        .ok_or_else(ApiError::not_found)
}

fn new_login_key(user_id: &str, raw_key: &SecretString, now_ms: u64) -> LocalUserLoginKeyRecord {
    LocalUserLoginKeyRecord {
        id: format!("userkey_{}", uuid::Uuid::new_v4().simple()),
        user_id: user_id.to_owned(),
        token_prefix: raw_key.expose_secret().chars().take(16).collect(),
        secret_hash: auth::hash(raw_key.expose_secret()),
        created_at_ms: now_ms,
        expires_at_ms: FAR_FUTURE_EXPIRY_MS,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

fn normalize_email(value: &str) -> Result<String, ApiError> {
    let email = value.trim().to_lowercase();
    let valid = (3..=254).contains(&email.len())
        && !email
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        && email
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && !domain.is_empty());
    if valid {
        Ok(email)
    } else {
        Err(ApiError::invalid_request())
    }
}

fn summary(listing: LocalUserListing) -> Result<LocalUserSummary, ApiError> {
    Ok(LocalUserSummary {
        created_at: transfer_grants::format_expiry(listing.user.created_at_ms)?,
        display_name: listing.user.display_name,
        email: listing.user.email,
        id: listing.user.id,
        login_key_prefix: listing.active_key_prefix,
        status: api_status(listing.user.status),
        workspace_id: listing.user.workspace_id,
    })
}

const fn api_status(status: LocalUserStatus) -> blobyard_api_client::LocalUserStatus {
    match status {
        LocalUserStatus::Active => blobyard_api_client::LocalUserStatus::Active,
        LocalUserStatus::Deactivated => blobyard_api_client::LocalUserStatus::Deactivated,
    }
}

#[cfg(test)]
#[path = "api_local_users_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "api_local_users_edge_tests.rs"]
mod edge_tests;
