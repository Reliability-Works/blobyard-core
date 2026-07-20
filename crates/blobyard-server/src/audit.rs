use crate::api::AppState;
use crate::auth::Principal;
use crate::error::ApiError;
use crate::response::{Success, success};
use crate::transfer_grants;
use axum::{
    Json, Router,
    extract::{Query, State, rejection::QueryRejection},
    routing::get,
};
use blobyard_contract::{
    AuditEventRecord, AuditValue, LocalApiTokenRecord, NewAuditEvent, ProjectRecord,
    WorkspaceRecord,
};
use blobyard_core::Slug;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/v1/audit", get(list))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditQuery {
    workspace: Slug,
    cursor: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditPageResponse {
    items: Vec<AuditResponse>,
    next_cursor: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditResponse {
    id: String,
    actor: String,
    action: String,
    request_id: String,
    target_type: String,
    metadata: BTreeMap<String, serde_json::Value>,
    created_at: String,
}

async fn list(
    State(state): State<AppState>,
    principal: Principal,
    query: Result<Query<AuditQuery>, QueryRejection>,
) -> Result<Json<Success<AuditPageResponse>>, ApiError> {
    principal.require("audit:read")?;
    let Query(query) = query.map_err(|_error| ApiError::invalid_request())?;
    let workspace = state
        .repository
        .workspace_by_slug(&query.workspace)
        .map_err(ApiError::from_repository)?;
    if workspace.id != principal.0.workspace_id {
        return Err(ApiError::not_found());
    }
    let cursor = query
        .cursor
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_error| ApiError::invalid_request())
        })
        .transpose()?;
    let page = state
        .repository
        .list_audit(&workspace.id, cursor, 50)
        .map_err(ApiError::from_repository)?;
    let items = page
        .items
        .into_iter()
        .map(AuditResponse::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(success(AuditPageResponse {
        items,
        next_cursor: page.next_before.map(|value| value.to_string()),
    }))
}

impl TryFrom<AuditEventRecord> for AuditResponse {
    type Error = ApiError;

    fn try_from(value: AuditEventRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            actor: value.actor,
            action: value.action,
            request_id: value.request_id,
            target_type: value.target_type,
            metadata: value
                .metadata
                .into_iter()
                .map(|(name, value)| (name, json_value(value)))
                .collect(),
            created_at: transfer_grants::format_expiry(value.created_at_ms)?,
        })
    }
}

fn json_value(value: AuditValue) -> serde_json::Value {
    match value {
        AuditValue::String(value) => serde_json::Value::String(value),
        AuditValue::Number(value) => serde_json::Value::from(value),
        AuditValue::Boolean(value) => serde_json::Value::from(value),
        AuditValue::Null => serde_json::Value::Null,
    }
}

pub(crate) fn record_action(
    state: &AppState,
    principal: &LocalApiTokenRecord,
    action: &str,
    target_type: &str,
    metadata: Vec<(String, AuditValue)>,
) -> Result<(), ApiError> {
    record_action_at(
        state,
        principal,
        action,
        target_type,
        metadata,
        transfer_grants::now_ms(),
    )
}

fn record_action_at(
    state: &AppState,
    principal: &LocalApiTokenRecord,
    action: &str,
    target_type: &str,
    metadata: Vec<(String, AuditValue)>,
    now: Result<u64, ApiError>,
) -> Result<(), ApiError> {
    let created_at_ms = now?;
    state
        .repository
        .record_audit(&action_event(
            principal,
            action,
            target_type,
            metadata,
            created_at_ms,
        ))
        .map_err(ApiError::from_repository)
}

fn action_event(
    principal: &LocalApiTokenRecord,
    action: &str,
    target_type: &str,
    metadata: Vec<(String, AuditValue)>,
    created_at_ms: u64,
) -> NewAuditEvent {
    event(
        principal.workspace_id.clone(),
        principal.id.clone(),
        action,
        target_type,
        metadata,
        created_at_ms,
    )
}

pub(crate) fn event(
    workspace_id: String,
    actor: String,
    action: &str,
    target_type: &str,
    metadata: Vec<(String, AuditValue)>,
    created_at_ms: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{}", uuid::Uuid::new_v4().simple()),
        workspace_id,
        actor,
        action: action.to_owned(),
        request_id: crate::error::request_id(),
        target_type: target_type.to_owned(),
        metadata,
        created_at_ms,
    }
}

pub(crate) fn bootstrap_exchanged(
    state: &AppState,
    principal: &LocalApiTokenRecord,
) -> Result<(), ApiError> {
    record_action(
        state,
        principal,
        "auth.bootstrap_exchanged",
        "api_token",
        vec![(
            "tokenId".to_owned(),
            AuditValue::String(principal.id.clone()),
        )],
    )
}

pub(crate) fn api_token_created_event(
    principal: &LocalApiTokenRecord,
    token: &LocalApiTokenRecord,
) -> NewAuditEvent {
    action_event(
        principal,
        "api_token.created",
        "api_token",
        vec![("tokenId".to_owned(), AuditValue::String(token.id.clone()))],
        token.created_at_ms,
    )
}

pub(crate) fn api_token_revoked_event(
    principal: &LocalApiTokenRecord,
    token_id: &str,
    revoked_at_ms: u64,
) -> NewAuditEvent {
    action_event(
        principal,
        "api_token.revoked",
        "api_token",
        vec![(
            "tokenId".to_owned(),
            AuditValue::String(token_id.to_owned()),
        )],
        revoked_at_ms,
    )
}

pub(crate) fn cli_session_revoked_event(
    principal: &LocalApiTokenRecord,
    session_id: &str,
    revoked_at_ms: u64,
) -> NewAuditEvent {
    action_event(
        principal,
        "cli.session_revoked",
        "cli_session",
        vec![(
            "sessionId".to_owned(),
            AuditValue::String(session_id.to_owned()),
        )],
        revoked_at_ms,
    )
}

pub(crate) fn workspace_created(
    state: &AppState,
    principal: &LocalApiTokenRecord,
    workspace: &WorkspaceRecord,
) -> Result<(), ApiError> {
    record_action(
        state,
        principal,
        "workspace.created",
        "workspace",
        vec![(
            "workspaceId".to_owned(),
            AuditValue::String(workspace.id.clone()),
        )],
    )
}

pub(crate) fn workspace_renamed_event(
    principal: &LocalApiTokenRecord,
    previous_slug: &Slug,
    created_at_ms: u64,
) -> NewAuditEvent {
    action_event(
        principal,
        "workspace.renamed",
        "workspace",
        vec![(
            "previousSlug".to_owned(),
            AuditValue::String(previous_slug.to_string()),
        )],
        created_at_ms,
    )
}

pub(crate) fn project_created(
    state: &AppState,
    principal: &LocalApiTokenRecord,
    project: &ProjectRecord,
) -> Result<(), ApiError> {
    record_action(
        state,
        principal,
        "project.created",
        "project",
        vec![(
            "projectId".to_owned(),
            AuditValue::String(project.id.clone()),
        )],
    )
}

#[cfg(test)]
#[path = "audit_tests.rs"]
mod tests;

#[cfg(any(test, feature = "test-seams"))]
#[path = "audit_seams.rs"]
/// Test-only entry point for a deterministic audit clock failure.
pub mod test_seams;
