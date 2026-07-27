use crate::{
    api::AppState,
    error::ApiError,
    yard_session_contracts::{self, ContinuationClaims},
};
use axum::{body::Body, http::Response};
use blobyard_contract::{
    NewAuditEvent, NewYardContinuation, RepositoryError, YARD_EXCHANGE_CODE_LIFETIME_MS,
    YardEnvironmentKind, YardEnvironmentRecord, YardEnvironmentStatus, YardGuestInviteRecord,
    YardGuestLoginKeyRecord, YardSubjectKind, YardSubjectRecord, yard_guest_audit_metadata,
};
use blobyard_core::{GeneratedSecretKind, SecretString};

pub(super) fn invitation(
    state: &AppState,
    token: &SecretString,
    continuation: &SecretString,
    claims: &ContinuationClaims,
    invitation: &YardGuestInviteRecord,
    now: u64,
) -> Result<Response<Body>, ApiError> {
    let Some(environment_id) = acceptance_environment(state, invitation)? else {
        return Ok(super::page::invalid_link());
    };
    let guest_key = crate::auth::generate_token(GeneratedSecretKind::YardGuestLoginKey);
    let code = crate::auth::generate_token(GeneratedSecretKind::YardExchangeCode);
    let subject = guest_subject(invitation, now);
    let key = guest_key_record(invitation, &subject, &guest_key, now);
    let durable = guest_continuation(
        invitation,
        &subject,
        continuation,
        claims,
        &code,
        environment_id,
        now,
    )?;
    let event = acceptance_event(invitation, &subject.id, now);
    let exchange_target = exchange_target(&state.web_yard_origin, claims.host_label())?;
    let response = super::page::accepted(
        guest_key.expose_secret(),
        &exchange_target,
        code.expose_secret(),
    );
    match state.repository.accept_yard_guest_invite(
        &crate::auth::hash(token.expose_secret()),
        &subject,
        &key,
        &durable,
        &event,
        now,
    ) {
        Ok(_accepted) => Ok(response),
        Err(error) => acceptance_error(error),
    }
}

fn acceptance_environment(
    state: &AppState,
    invitation: &YardGuestInviteRecord,
) -> Result<Option<String>, ApiError> {
    let environments = match state.repository.list_yard_environments(&invitation.yard_id) {
        Ok(environments) => environments,
        Err(RepositoryError::NotFound | RepositoryError::InvalidInput) => return Ok(None),
        Err(_) => return Err(ApiError::internal()),
    };
    Ok(select_environment(environments, invitation))
}

fn select_environment(
    environments: Vec<YardEnvironmentRecord>,
    invitation: &YardGuestInviteRecord,
) -> Option<String> {
    environments
        .into_iter()
        .find(|environment| {
            environment.status == YardEnvironmentStatus::Active
                && environment.kind == YardEnvironmentKind::Production
                && invitation
                    .environment_id
                    .as_ref()
                    .is_none_or(|id| id == &environment.id)
        })
        .map(|environment| environment.id)
}

fn acceptance_error(error: RepositoryError) -> Result<Response<Body>, ApiError> {
    match error {
        RepositoryError::NotFound | RepositoryError::Conflict | RepositoryError::InvalidInput => {
            Ok(super::page::invalid_link())
        }
        RepositoryError::SchemaTooNew | RepositoryError::Unavailable => Err(ApiError::internal()),
    }
}

fn guest_subject(invitation: &YardGuestInviteRecord, now: u64) -> YardSubjectRecord {
    YardSubjectRecord {
        id: format!("guest_{}", uuid::Uuid::new_v4().simple()),
        kind: YardSubjectKind::Guest,
        workspace_id: invitation.workspace_id.clone(),
        local_user_id: None,
        invitation_id: Some(invitation.id.clone()),
        created_at_ms: now,
        revoked_at_ms: None,
    }
}

fn guest_key_record(
    invitation: &YardGuestInviteRecord,
    subject: &YardSubjectRecord,
    raw: &SecretString,
    now: u64,
) -> YardGuestLoginKeyRecord {
    YardGuestLoginKeyRecord {
        id: format!("ygk_{}", uuid::Uuid::new_v4().simple()),
        subject_id: subject.id.clone(),
        invitation_id: invitation.id.clone(),
        workspace_id: invitation.workspace_id.clone(),
        token_prefix: raw.expose_secret().chars().take(16).collect(),
        secret_hash: crate::auth::hash(raw.expose_secret()),
        created_at_ms: now,
        expires_at_ms: invitation.expires_at_ms,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

fn guest_continuation(
    invitation: &YardGuestInviteRecord,
    subject: &YardSubjectRecord,
    signed: &SecretString,
    claims: &ContinuationClaims,
    code: &SecretString,
    environment_id: String,
    now: u64,
) -> Result<NewYardContinuation, ApiError> {
    Ok(NewYardContinuation {
        id: format!("yardcont_{}", uuid::Uuid::new_v4().simple()),
        continuation_hash: crate::auth::hash(signed.expose_secret()),
        code_hash: crate::auth::hash(code.expose_secret()),
        yard_id: invitation.yard_id.clone(),
        environment_id,
        host_label: claims.host_label().to_owned(),
        user_id: subject.id.clone(),
        return_path: claims.return_path().to_owned(),
        created_at_ms: now,
        expires_at_ms: exchange_expiry(now)?,
    })
}

fn exchange_expiry(now: u64) -> Result<u64, ApiError> {
    now.checked_add(YARD_EXCHANGE_CODE_LIFETIME_MS)
        .ok_or_else(ApiError::internal)
}

fn acceptance_event(
    invitation: &YardGuestInviteRecord,
    subject_id: &str,
    now: u64,
) -> NewAuditEvent {
    crate::audit::event(
        invitation.workspace_id.clone(),
        subject_id.to_owned(),
        "yard.guest_invite.accepted",
        "yard_guest_invite",
        yard_guest_audit_metadata(invitation, Some(subject_id)),
        now,
    )
}

fn exchange_target(origin: &str, host_label: &str) -> Result<String, ApiError> {
    let root = yard_session_contracts::yard_url(origin, host_label)?;
    Ok(format!("{root}/.blobyard/session/exchange"))
}

#[cfg(test)]
#[path = "yard_invite_authority_tests.rs"]
mod authority_tests;
#[cfg(test)]
#[path = "yard_invite_exchange_expiry_tests.rs"]
mod exchange_expiry_tests;
#[cfg(test)]
#[path = "yard_invite_accept_tests.rs"]
mod tests;
