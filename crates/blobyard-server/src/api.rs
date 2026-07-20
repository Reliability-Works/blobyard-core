use crate::Repository;
use crate::{
    auth::Principal,
    error::ApiError,
    response::{Page, Success, page, success},
    slug,
};
use axum::{
    Json, Router,
    extract::{
        Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    routing::{get, post},
};
use blobyard_contract::{ObjectStorage, ProjectRecord, WorkspaceRecord};
use blobyard_core::{SecretString, Slug};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

#[path = "api_bootstrap.rs"]
mod bootstrap;
#[path = "api_query.rs"]
mod query_helpers;

include!("api_credential_revoke.rs");

use bootstrap::exchange_bootstrap;
use query_helpers::reject_cursor;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) repository: Arc<dyn Repository>,
    pub(crate) storage: Arc<dyn ObjectStorage>,
    pub(crate) capability_key: Arc<SecretString>,
    pub(crate) public_origin: String,
    pub(crate) web_yard_origin: String,
    pub(crate) staging_directory: PathBuf,
    pub(crate) default_workspace: WorkspaceRecord,
    pub(crate) oidc_verifier: Arc<dyn crate::oidc::GithubOidcVerifier>,
}

pub(crate) fn router(
    repository: Arc<dyn Repository>,
    storage: Arc<dyn ObjectStorage>,
    workspace: WorkspaceRecord,
    capability_key: Arc<SecretString>,
    public_origin: String,
    web_yard_origin: String,
    staging_directory: PathBuf,
) -> Router {
    let state = AppState {
        repository,
        storage,
        capability_key,
        public_origin,
        web_yard_origin,
        staging_directory,
        default_workspace: workspace,
        oidc_verifier: Arc::new(crate::oidc::RemoteGithubOidcVerifier::new()),
    };
    router_with_state(state)
}

pub(crate) fn router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/v1/health", get(crate::response::health))
        .route("/v1/bootstrap/exchange", post(exchange_bootstrap))
        .route("/v1/cli/whoami", get(who_am_i))
        .route(
            "/v1/workspaces",
            get(list_workspaces).post(create_workspace),
        )
        .route("/v1/projects", get(list_projects).post(create_project))
        .merge(crate::api_cli_sessions::routes())
        .merge(crate::api_ci_trusts::routes())
        .merge(crate::api_ci_exchange::routes())
        .merge(crate::api_tokens::routes())
        .merge(crate::api_workspace_rename::routes())
        .merge(crate::objects::routes())
        .merge(crate::retention::routes())
        .merge(crate::audit::routes())
        .merge(crate::transfers::routes())
        .merge(crate::shares::routes())
        .merge(crate::inboxes::routes())
        .merge(crate::previews::routes())
        .merge(crate::yards::routes())
        .fallback(crate::yards::public_fallback)
        .with_state(state)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Identity {
    principal_type: &'static str,
    principal_id: String,
    display_name: String,
    email: Option<String>,
    scopes: Vec<String>,
    default_workspace: WorkspaceResponse,
}

pub(crate) async fn who_am_i(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<Success<Identity>>, ApiError> {
    let record = principal.0;
    let workspace = crate::transfer_grants::workspace_by_id(&state, &record.workspace_id)?;
    Ok(success(Identity {
        principal_type: if record.id.starts_with("machine_") {
            "ci"
        } else {
            "cli"
        },
        principal_id: record.id,
        display_name: record.name,
        email: None,
        scopes: record.scopes,
        default_workspace: WorkspaceResponse::from(workspace),
    }))
}

#[derive(Deserialize)]
struct CursorQuery {
    cursor: Option<String>,
}

#[derive(Serialize)]
pub(super) struct WorkspaceResponse {
    id: String,
    name: String,
    slug: Slug,
}

impl From<WorkspaceRecord> for WorkspaceResponse {
    fn from(value: WorkspaceRecord) -> Self {
        Self {
            id: value.id,
            name: value.name,
            slug: value.slug,
        }
    }
}

async fn list_workspaces(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<CursorQuery>, QueryRejection>,
) -> Result<Json<Success<Page<WorkspaceResponse>>>, ApiError> {
    principal.require("workspace:read")?;
    let query = match query {
        Ok(Query(query)) => query,
        Err(_error) => return Err(ApiError::invalid_request()),
    };
    reject_cursor(query.cursor.as_deref())?;
    let items = state
        .repository
        .list_workspaces()
        .map_err(ApiError::from_repository)?
        .into_iter()
        .map(WorkspaceResponse::from)
        .collect();
    Ok(success(page(items)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateWorkspaceRequest {
    name: String,
}

async fn create_workspace(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<CreateWorkspaceRequest>, JsonRejection>,
) -> Result<Json<Success<WorkspaceResponse>>, ApiError> {
    principal.require("project:write")?;
    let Json(request) = payload.map_err(|_error| ApiError::invalid_request())?;
    slug::validate_name(&request.name)?;
    let record = WorkspaceRecord {
        id: format!("workspace_{}", uuid::Uuid::new_v4().simple()),
        slug: slug::from_name(&request.name).ok_or_else(ApiError::invalid_request)?,
        name: request.name,
    };
    state
        .repository
        .create_workspace(&record)
        .map_err(ApiError::from_repository)?;
    crate::audit::workspace_created(&state, &principal.0, &record)?;
    Ok(success(WorkspaceResponse::from(record)))
}

#[derive(Deserialize)]
struct ProjectsQuery {
    workspace: String,
    cursor: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectResponse {
    id: String,
    workspace_slug: Slug,
    slug: Slug,
    name: String,
}

async fn list_projects(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<ProjectsQuery>, QueryRejection>,
) -> Result<Json<Success<Page<ProjectResponse>>>, ApiError> {
    principal.require("project:read")?;
    let Query(query) = ApiError::invalid_request_result(query)?;
    reject_cursor(query.cursor.as_deref())?;
    let workspace_slug = slug::parse(query.workspace)?;
    let workspace = state
        .repository
        .workspace_by_slug(&workspace_slug)
        .map_err(ApiError::from_repository)?;
    let items = state
        .repository
        .list_projects(&workspace.id)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .map(|project| ProjectResponse::new(project, workspace_slug.clone()))
        .collect();
    Ok(success(page(items)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateProjectRequest {
    workspace: String,
    name: String,
}

async fn create_project(
    State(state): State<AppState>,
    principal: Principal,
    payload: Result<Json<CreateProjectRequest>, JsonRejection>,
) -> Result<Json<Success<ProjectResponse>>, ApiError> {
    principal.require("project:write")?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    slug::validate_name(&request.name)?;
    let workspace_slug = slug::parse(request.workspace)?;
    let workspace = state
        .repository
        .workspace_by_slug(&workspace_slug)
        .map_err(ApiError::from_repository)?;
    let record = ProjectRecord {
        id: format!("project_{}", uuid::Uuid::new_v4().simple()),
        workspace_id: workspace.id,
        slug: slug::from_name(&request.name).ok_or_else(ApiError::invalid_request)?,
        name: request.name,
    };
    state
        .repository
        .create_project(&record)
        .map_err(ApiError::from_repository)?;
    crate::audit::project_created(&state, &principal.0, &record)?;
    Ok(success(ProjectResponse::new(record, workspace_slug)))
}

impl ProjectResponse {
    fn new(value: ProjectRecord, workspace_slug: Slug) -> Self {
        Self {
            id: value.id,
            workspace_slug,
            slug: value.slug,
            name: value.name,
        }
    }
}

#[cfg(test)]
async fn not_found() -> ApiError {
    ApiError::not_found()
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "api_contract_tests.rs"]
mod contract_tests;
