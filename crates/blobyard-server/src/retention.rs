use crate::{
    api::AppState,
    auth::Principal,
    error::ApiError,
    response::{Success, success},
    transfer_grants,
};
use axum::{
    Json, Router,
    extract::{
        Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    routing::get,
};
use blobyard_contract::{
    NewAuditEvent, RepositoryError, RetentionOverview, RetentionPolicyRecord, RetentionRunRecord,
};
use blobyard_core::Slug;
use serde::{Deserialize, Serialize};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/retention",
            get(get_policy).put(set_policy).delete(clear_policy),
        )
        .route("/v1/retention/overview", get(overview))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetentionQuery {
    workspace: Slug,
    project: Slug,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SetRetentionRequest {
    workspace: Slug,
    project: Slug,
    keep_latest: u32,
    branch: Option<String>,
    path: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PolicyResponse {
    keep_latest: u32,
    branch_glob: Option<String>,
    path_glob: Option<String>,
}

#[derive(Serialize)]
struct ClearResponse {
    cleared: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OverviewResponse {
    policy: Option<PolicyResponse>,
    last_run: Option<RunResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunResponse {
    candidate_count: u64,
    deleted_count: u64,
    status: String,
    started_at: u64,
    completed_at: Option<u64>,
    error_summary: Option<String>,
}

async fn get_policy(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<RetentionQuery>, QueryRejection>,
) -> Result<Json<Success<PolicyResponse>>, ApiError> {
    principal.require("retention:manage")?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    let project = project(&state, &principal, &query.workspace, &query.project)?;
    match state.repository.retention_policy(&project.id) {
        Ok(policy) => Ok(success(policy.into())),
        Err(error) => Err(ApiError::from_repository(error)),
    }
}

async fn set_policy(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<SetRetentionRequest>, JsonRejection>,
) -> Result<Json<Success<PolicyResponse>>, ApiError> {
    principal.require("retention:manage")?;
    let Json(request) = payload.map_err(|_error| ApiError::invalid_request())?;
    set_policy_at(&state, &principal, request, transfer_grants::now_ms())
}

fn set_policy_at(
    state: &AppState,
    principal: &Principal,
    request: SetRetentionRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<PolicyResponse>>, ApiError> {
    let project = project(state, principal, &request.workspace, &request.project)?;
    let now = now?;
    let created_at_ms = match state.repository.retention_policy(&project.id) {
        Ok(existing) => existing.created_at_ms,
        Err(RepositoryError::NotFound) => now,
        Err(error) => return Err(ApiError::from_repository(error)),
    };
    let policy = RetentionPolicyRecord {
        project_id: project.id,
        keep_latest: request.keep_latest,
        path_glob: request.path,
        branch_glob: request.branch,
        created_at_ms,
        updated_at_ms: now,
    };
    state
        .repository
        .set_retention(
            &policy,
            &event(principal, "retention.policy_set", "retention_policy", now),
        )
        .map_err(ApiError::from_repository)?;
    Ok(success(policy.into()))
}

async fn clear_policy(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<RetentionQuery>, QueryRejection>,
) -> Result<Json<Success<ClearResponse>>, ApiError> {
    principal.require("retention:manage")?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    clear_policy_at(&state, &principal, &query, transfer_grants::now_ms())
}

fn clear_policy_at(
    state: &AppState,
    principal: &Principal,
    query: &RetentionQuery,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<ClearResponse>>, ApiError> {
    let project = project(state, principal, &query.workspace, &query.project)?;
    let now = now?;
    let cleared = state
        .repository
        .clear_retention(
            &project.id,
            now,
            &event(
                principal,
                "retention.policy_cleared",
                "retention_policy",
                now,
            ),
        )
        .map_err(ApiError::from_repository)?;
    if !cleared {
        return Err(ApiError::not_found());
    }
    Ok(success(ClearResponse { cleared }))
}

async fn overview(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<RetentionQuery>, QueryRejection>,
) -> Result<Json<Success<OverviewResponse>>, ApiError> {
    principal.require("retention:manage")?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    let project = project(&state, &principal, &query.workspace, &query.project)?;
    let overview = state
        .repository
        .retention_overview(&project.id)
        .map_err(ApiError::from_repository)?;
    Ok(success(overview.into()))
}

fn project(
    state: &AppState,
    principal: &Principal,
    workspace: &Slug,
    project: &Slug,
) -> Result<blobyard_contract::ProjectRecord, ApiError> {
    transfer_grants::resolve_project_slugs(state, &principal.0.workspace_id, workspace, project)
}

fn event(
    principal: &Principal,
    action: &str,
    target_type: &str,
    created_at_ms: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{}", uuid::Uuid::new_v4().simple()),
        workspace_id: principal.0.workspace_id.clone(),
        actor: principal.0.id.clone(),
        action: action.to_owned(),
        request_id: crate::error::request_id(),
        target_type: target_type.to_owned(),
        metadata: Vec::new(),
        created_at_ms,
    }
}

impl From<RetentionPolicyRecord> for PolicyResponse {
    fn from(value: RetentionPolicyRecord) -> Self {
        Self {
            keep_latest: value.keep_latest,
            branch_glob: value.branch_glob,
            path_glob: value.path_glob,
        }
    }
}

impl From<RetentionOverview> for OverviewResponse {
    fn from(value: RetentionOverview) -> Self {
        Self {
            policy: value.policy.map(PolicyResponse::from),
            last_run: value.last_run.map(RunResponse::from),
        }
    }
}

impl From<RetentionRunRecord> for RunResponse {
    fn from(value: RetentionRunRecord) -> Self {
        Self {
            candidate_count: value.candidate_count,
            deleted_count: value.deleted_count,
            status: value.status,
            started_at: value.started_at_ms,
            completed_at: value.completed_at_ms,
            error_summary: value.error_summary,
        }
    }
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod tests;

#[cfg(any(test, feature = "test-seams"))]
#[path = "retention_seams.rs"]
/// Test-only entry points for deterministic retention clock failures.
pub mod test_seams;
