use crate::{
    api::AppState,
    api_ci_trusts,
    auth::{self},
    error::{ApiError, request_id},
    oidc::OidcVerificationError,
    response::{Success, success_with_request},
    transfer_grants,
};
use axum::{
    Json, Router,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, header::AUTHORIZATION},
    routing::post,
};
use blobyard_contract::{
    AuditValue, MachineSessionMintResult, NewAuditEvent, NewMachineSession, ProjectRecord,
    WorkspaceRecord,
};
use blobyard_core::{GeneratedSecretKind, SecretString, Slug};
use serde::{Deserialize, Serialize};

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/v1/ci/github/oidc/exchange", post(exchange))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExchangeRequest {
    actions: Vec<String>,
    project: Slug,
    workspace: Option<Slug>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExchangeResponse {
    access_token: SecretString,
    expires_in_seconds: u64,
    scopes: Vec<String>,
}

async fn exchange(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<ExchangeRequest>, JsonRejection>,
) -> Result<Json<Success<ExchangeResponse>>, ApiError> {
    exchange_at(&state, &headers, payload, transfer_grants::now_ms()).await
}

async fn exchange_at(
    state: &AppState,
    headers: &HeaderMap,
    payload: Result<Json<ExchangeRequest>, JsonRejection>,
    now_ms: Result<u64, ApiError>,
) -> Result<Json<Success<ExchangeResponse>>, ApiError> {
    let assertion = oidc_assertion(headers)?;
    let Json(request) = ApiError::invalid_request_result(payload)?;
    let actions = api_ci_trusts::actions(request.actions)?;
    let now_ms = now_ms?;
    let identity = state
        .oidc_verifier
        .verify(assertion.expose_secret(), &state.public_origin, now_ms)
        .await
        .map_err(oidc_error)?;
    let workspace_slug = request
        .workspace
        .unwrap_or_else(|| state.default_workspace.slug.clone());
    let raw_token = auth::generate_token(GeneratedSecretKind::MachineToken);
    let request_id = request_id();
    let target = event_target(state, &workspace_slug, &request.project);
    let session = NewMachineSession {
        id: format!("machine_{}", uuid::Uuid::new_v4().simple()),
        token_prefix: raw_token.expose_secret().chars().take(16).collect(),
        secret_hash: auth::hash(raw_token.expose_secret()),
        oidc_token_hash: auth::hash(assertion.expose_secret()),
        identity,
        workspace: Some(workspace_slug.to_string()),
        project: request.project.to_string(),
        actions,
        now_ms,
    };
    let event = session_event(&session, &target, &request_id);
    let result = state
        .repository
        .mint_machine_session(&session, &event)
        .map_err(ApiError::from_repository)?;
    response(result, raw_token, now_ms, request_id)
}

fn response(
    result: MachineSessionMintResult,
    raw_token: SecretString,
    now_ms: u64,
    request_id: String,
) -> Result<Json<Success<ExchangeResponse>>, ApiError> {
    match result {
        MachineSessionMintResult::Minted(record) => {
            let record = *record;
            Ok(success_with_request(
                ExchangeResponse {
                    access_token: raw_token,
                    expires_in_seconds: record.expires_at_ms.saturating_sub(now_ms).div_ceil(1_000),
                    scopes: record
                        .actions
                        .into_iter()
                        .map(|action| action.as_str().to_owned())
                        .collect(),
                },
                request_id,
            ))
        }
        MachineSessionMintResult::Forbidden => Err(ApiError::forbidden()),
        MachineSessionMintResult::Replayed => Err(ApiError::invalid_token()),
        MachineSessionMintResult::RateLimited {
            retry_after_seconds,
        } => Err(ApiError::rate_limited(retry_after_seconds)),
    }
}

fn oidc_assertion(headers: &HeaderMap) -> Result<SecretString, ApiError> {
    let value = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(ApiError::invalid_token)?;
    if value.split('.').count() != 3 {
        return Err(ApiError::invalid_token());
    }
    ApiError::invalid_token_result(SecretString::new(value.to_owned()))
}

fn event_target(state: &AppState, workspace: &Slug, project: &Slug) -> EventTarget {
    let resolved = state
        .repository
        .workspace_by_slug(workspace)
        .and_then(|workspace| {
            state
                .repository
                .project_by_slug(&workspace.id, project)
                .map(|project| (workspace, project))
        });
    resolved.map_or_else(
        |_error| EventTarget {
            project_id: "project_unmatched".to_owned(),
            workspace_id: "workspace_unmatched".to_owned(),
        },
        |(workspace, project)| EventTarget::new(workspace, project),
    )
}

struct EventTarget {
    project_id: String,
    workspace_id: String,
}

impl EventTarget {
    fn new(workspace: WorkspaceRecord, project: ProjectRecord) -> Self {
        Self {
            project_id: project.id,
            workspace_id: workspace.id,
        }
    }
}

fn session_event(
    session: &NewMachineSession,
    target: &EventTarget,
    request_id: &str,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{}", uuid::Uuid::new_v4().simple()),
        workspace_id: target.workspace_id.clone(),
        actor: format!("github:{}", session.identity.repository),
        action: "ci.token_minted".to_owned(),
        request_id: request_id.to_owned(),
        target_type: "project".to_owned(),
        metadata: vec![
            (
                "repository".to_owned(),
                AuditValue::String(session.identity.repository.clone()),
            ),
            (
                "targetId".to_owned(),
                AuditValue::String(target.project_id.clone()),
            ),
        ],
        created_at_ms: session.now_ms,
    }
}

const fn oidc_error(error: OidcVerificationError) -> ApiError {
    match error {
        OidcVerificationError::Invalid => ApiError::invalid_token(),
        OidcVerificationError::ProviderUnavailable => ApiError::provider_unavailable(),
    }
}

#[cfg(any(test, feature = "test-seams"))]
#[doc(hidden)]
#[path = "api_ci_exchange_seams.rs"]
pub mod test_seams;

#[cfg(test)]
#[path = "api_ci_exchange_tests.rs"]
mod tests;
