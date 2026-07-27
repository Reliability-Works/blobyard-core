use crate::{
    api::AppState,
    error::ApiError,
    yard_session_contracts::{self, ContinuationClaims},
};
use blobyard_contract::{
    RepositoryError, WebYardRecord, WebYardStatus, YardDeployRecord, YardDeployStatus,
    YardGuestInviteRecord,
};
use blobyard_core::SecretString;

pub(super) type ResolvedInvite = (
    SecretString,
    SecretString,
    ContinuationClaims,
    YardGuestInviteRecord,
);

pub(super) fn query(
    state: &AppState,
    query: Option<&str>,
    now: u64,
) -> Result<Option<ResolvedInvite>, ApiError> {
    let Some((token, continuation)) = super::invite_parameters(query.unwrap_or_default()) else {
        return Ok(None);
    };
    values(state, token, continuation, now)
}

pub(super) fn values(
    state: &AppState,
    token: String,
    continuation: String,
    now: u64,
) -> Result<Option<ResolvedInvite>, ApiError> {
    let Ok(token) = SecretString::new(token) else {
        return Ok(None);
    };
    if !yard_session_contracts::has_token_shape(token.expose_secret(), "bygi_") {
        return Ok(None);
    }
    let Ok(continuation) = SecretString::new(continuation) else {
        return Ok(None);
    };
    let Ok(claims) =
        yard_session_contracts::verify_invitation(&state.yard_continuation_key, &continuation, now)
    else {
        return Ok(None);
    };
    let Some(invitation) = resolved_invitation(
        state
            .repository
            .pending_yard_guest_invite_by_token(&crate::auth::hash(token.expose_secret()), now),
    )?
    else {
        return Ok(None);
    };
    if !target_is_live(state, &invitation, claims.host_label())? {
        return Ok(None);
    }
    Ok(Some((token, continuation, claims, invitation)))
}

fn resolved_invitation(
    result: Result<YardGuestInviteRecord, RepositoryError>,
) -> Result<Option<YardGuestInviteRecord>, ApiError> {
    resolved_record(result)
}

fn resolved_record<T>(result: Result<T, RepositoryError>) -> Result<Option<T>, ApiError> {
    match result {
        Ok(record) => Ok(Some(record)),
        Err(RepositoryError::NotFound | RepositoryError::InvalidInput) => Ok(None),
        Err(
            RepositoryError::Conflict
            | RepositoryError::SchemaTooNew
            | RepositoryError::Unavailable,
        ) => Err(ApiError::internal()),
    }
}

fn target_is_live(
    state: &AppState,
    invitation: &YardGuestInviteRecord,
    host_label: &str,
) -> Result<bool, ApiError> {
    let Some(yard) = resolved_yard(state.repository.web_yard_by_id(&invitation.yard_id))? else {
        return Ok(false);
    };
    if !yard_matches_invitation(&yard, invitation) {
        return Ok(false);
    }
    if yard.host_label == host_label {
        return Ok(true);
    }
    resolved_deploys(state.repository.list_yard_deploys(&yard.id), host_label)
}

fn resolved_yard(
    result: Result<WebYardRecord, RepositoryError>,
) -> Result<Option<WebYardRecord>, ApiError> {
    resolved_record(result)
}

fn yard_matches_invitation(yard: &WebYardRecord, invitation: &YardGuestInviteRecord) -> bool {
    yard.status == WebYardStatus::Active
        && yard.workspace_id == invitation.workspace_id
        && yard.project_id == invitation.project_id
}

fn resolved_deploys(
    result: Result<Vec<YardDeployRecord>, RepositoryError>,
    host_label: &str,
) -> Result<bool, ApiError> {
    match result {
        Ok(deploys) => Ok(deploys.into_iter().any(|deploy| {
            deploy.deployment_host_label == host_label
                && matches!(
                    deploy.status,
                    YardDeployStatus::Live | YardDeployStatus::Superseded
                )
        })),
        Err(RepositoryError::NotFound | RepositoryError::InvalidInput) => Ok(false),
        Err(
            RepositoryError::Conflict
            | RepositoryError::SchemaTooNew
            | RepositoryError::Unavailable,
        ) => Err(ApiError::internal()),
    }
}

#[cfg(test)]
#[path = "yard_invite_resolution_lifetime_tests.rs"]
mod lifetime_tests;
#[cfg(test)]
#[path = "yard_invite_resolution_tests.rs"]
mod tests;
