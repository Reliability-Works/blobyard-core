use crate::{
    api::{self, AppState, CredentialRevokeRequest, RevokeTarget},
    auth::Principal,
    error::ApiError,
    response::{Success, success},
};
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use blobyard_contract::LocalCliSessionRecord;
use serde::{Deserialize, Serialize};

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/v1/cli/sessions", get(list)).route(
        "/v1/cli/sessions/revoke",
        post(api::revoke::<RevokeRequest>),
    )
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionSummary {
    created_at: u64,
    id: String,
    last_used_at: Option<u64>,
    name: String,
    platform: String,
    version: String,
}

impl From<LocalCliSessionRecord> for SessionSummary {
    fn from(session: LocalCliSessionRecord) -> Self {
        Self {
            created_at: session.created_at_ms,
            id: session.id,
            last_used_at: session.last_used_at_ms,
            name: session.name,
            platform: session.platform,
            version: session.version,
        }
    }
}

async fn list(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<Success<Vec<SessionSummary>>>, ApiError> {
    principal.require("sessions:manage")?;
    let sessions = state
        .repository
        .list_cli_sessions(&principal.0.workspace_id)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .map(SessionSummary::from)
        .collect();
    Ok(success(sessions))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevokeRequest {
    session_id: String,
}

impl CredentialRevokeRequest for RevokeRequest {
    const REQUIRED_SCOPE: &'static str = "sessions:manage";

    fn into_target(self) -> RevokeTarget {
        RevokeTarget::CliSession(self.session_id)
    }
}

#[cfg(test)]
#[path = "api_cli_sessions_tests.rs"]
mod tests;
