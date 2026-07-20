use crate::{
    api::AppState,
    auth::Principal,
    error::{ApiError, request_id},
    response::{Success, success, success_with_request},
    transfer_grants,
};
use axum::{
    Json, Router,
    extract::{Query, State, rejection::JsonRejection},
    routing::{get, post},
};
use blobyard_contract::{CiAction, LocalCiTrustRecord, NewAuditEvent, RepositoryError};
use blobyard_core::Slug;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/ci/trusts", get(list).post(create))
        .route("/v1/ci/trusts/revoke", post(revoke))
}

#[derive(Deserialize)]
struct ListQuery {
    workspace: Slug,
}

async fn list(
    State(state): State<AppState>,
    principal: Principal,
    Query(query): Query<ListQuery>,
) -> Result<Json<Success<Vec<TrustSummary>>>, ApiError> {
    principal.require("ci:manage")?;
    let workspace = state
        .repository
        .workspace_by_slug(&query.workspace)
        .map_err(ApiError::from_repository)?;
    let trusts = state
        .repository
        .list_ci_trusts(&workspace.id)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .map(TrustSummary::from)
        .collect();
    Ok(success(trusts))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateRequest {
    allowed_actions: Vec<String>,
    allowed_ref_glob: String,
    environment: Option<String>,
    project: Option<Slug>,
    repository: String,
    workflow_path: String,
    workflow_ref: String,
    workspace: Slug,
}

async fn create(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<CreateRequest>, JsonRejection>,
) -> Result<Json<Success<TrustSummary>>, ApiError> {
    create_at(&state, &principal, payload, transfer_grants::now_ms())
}

fn create_at(
    state: &AppState,
    principal: &Principal,
    payload: Result<Json<CreateRequest>, JsonRejection>,
    now_ms: Result<u64, ApiError>,
) -> Result<Json<Success<TrustSummary>>, ApiError> {
    principal.require("ci:manage")?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    let now_ms = now_ms?;
    let workspace = state
        .repository
        .workspace_by_slug(&request.workspace)
        .map_err(ApiError::from_repository)?;
    let project_id = request
        .project
        .map(|project| {
            state
                .repository
                .project_by_slug(&workspace.id, &project)
                .map(|record| record.id)
                .map_err(ApiError::from_repository)
        })
        .transpose()?;
    let trust = LocalCiTrustRecord {
        id: format!("trust_{}", uuid::Uuid::new_v4().simple()),
        workspace_id: workspace.id,
        project_id,
        repository: request.repository.to_ascii_lowercase(),
        workflow_path: request.workflow_path,
        workflow_ref: request.workflow_ref,
        allowed_ref_glob: request.allowed_ref_glob,
        environment: request.environment.filter(|value| !value.is_empty()),
        allowed_actions: actions(request.allowed_actions)?,
        audience: state.public_origin.clone(),
        created_at_ms: now_ms,
        revoked_at_ms: None,
    };
    let request_id = request_id();
    let event = trust_event(principal, &trust, "ci.trust_created", &request_id, now_ms);
    state
        .repository
        .create_ci_trust(&trust, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success_with_request(TrustSummary::from(trust), request_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevokeRequest {
    trust_id: String,
}

async fn revoke(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<RevokeRequest>, JsonRejection>,
) -> Result<Json<Success<&'static str>>, ApiError> {
    revoke_at(&state, &principal, payload, transfer_grants::now_ms())
}

fn revoke_at(
    state: &AppState,
    principal: &Principal,
    payload: Result<Json<RevokeRequest>, JsonRejection>,
    now_ms: Result<u64, ApiError>,
) -> Result<Json<Success<&'static str>>, ApiError> {
    principal.require("ci:manage")?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    transfer_grants::validate_field(&request.trust_id)?;
    let trust = find_trust(state, &request.trust_id)?;
    let now_ms = now_ms?;
    let request_id = request_id();
    let event = trust_event(principal, &trust, "ci.trust_revoked", &request_id, now_ms);
    let revoked = state
        .repository
        .revoke_ci_trust(&trust.id, &trust.workspace_id, now_ms, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success_with_request(
        if revoked {
            "revoked"
        } else {
            "already_revoked"
        },
        request_id,
    ))
}

fn find_trust(state: &AppState, id: &str) -> Result<LocalCiTrustRecord, ApiError> {
    for workspace in state
        .repository
        .list_workspaces()
        .map_err(ApiError::from_repository)?
    {
        if let Some(trust) = state
            .repository
            .list_ci_trusts(&workspace.id)
            .map_err(ApiError::from_repository)?
            .into_iter()
            .find(|trust| trust.id == id)
        {
            return Ok(trust);
        }
    }
    Err(ApiError::from_repository(RepositoryError::NotFound))
}

pub(crate) fn actions(values: Vec<String>) -> Result<Vec<CiAction>, ApiError> {
    let actions = values
        .into_iter()
        .map(|value| CiAction::parse(&value).ok_or_else(ApiError::invalid_request))
        .collect::<Result<Vec<_>, _>>()?;
    let unique = actions.iter().copied().collect::<BTreeSet<_>>();
    if actions.is_empty() || actions.len() > 4 || unique.len() != actions.len() {
        Err(ApiError::invalid_request())
    } else {
        Ok(actions)
    }
}

fn trust_event(
    principal: &Principal,
    trust: &LocalCiTrustRecord,
    action: &str,
    request_id: &str,
    now_ms: u64,
) -> NewAuditEvent {
    blobyard_contract::ci_audit_event(blobyard_contract::NewCiAuditEvent {
        id: format!("audit_{}", uuid::Uuid::new_v4().simple()),
        workspace_id: trust.workspace_id.clone(),
        actor: principal.0.id.clone(),
        action: action.to_owned(),
        request_id: request_id.to_owned(),
        target_type: "ci_trust".to_owned(),
        target_id: trust.id.clone(),
        repository: trust.repository.clone(),
        created_at_ms: now_ms,
    })
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrustSummary {
    allowed_actions: Vec<String>,
    allowed_ref_glob: String,
    created_at: u64,
    environment: Option<String>,
    id: String,
    project_id: Option<String>,
    repository: String,
    revoked_at: Option<u64>,
    workflow_path: String,
    workflow_ref: String,
}

impl From<LocalCiTrustRecord> for TrustSummary {
    fn from(trust: LocalCiTrustRecord) -> Self {
        Self {
            allowed_actions: trust
                .allowed_actions
                .into_iter()
                .map(|action| action.as_str().to_owned())
                .collect(),
            allowed_ref_glob: trust.allowed_ref_glob,
            created_at: trust.created_at_ms,
            environment: trust.environment,
            id: trust.id,
            project_id: trust.project_id,
            repository: trust.repository,
            revoked_at: trust.revoked_at_ms,
            workflow_path: trust.workflow_path,
            workflow_ref: trust.workflow_ref,
        }
    }
}

#[cfg(any(test, feature = "test-seams"))]
#[doc(hidden)]
#[path = "api_ci_trusts_seams.rs"]
pub mod test_seams;

#[cfg(test)]
#[path = "api_ci_trusts_tests.rs"]
mod tests;
