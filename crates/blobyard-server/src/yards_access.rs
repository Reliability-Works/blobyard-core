use super::presentation::{
    api_visibility, domain_principal_kind, domain_visibility, grant_summary,
};
use super::read::authorize_yard;
use crate::{
    api::AppState,
    audit,
    auth::Principal,
    error::ApiError,
    response::{Success, success},
};
use axum::Json;
use blobyard_api_client::{
    EmptyResponse, GetYardAccessQuery, GrantYardAccessRequest, RevokeYardAccessRequest,
    SetYardVisibilityRequest, YardAccessGrantResponse, YardAccessResponse, YardVisibilityResponse,
};
use blobyard_contract::{AuditValue, NewYardAccessGrant, WebYardRecord, YardVisibility};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const MAXIMUM_PRINCIPAL_LENGTH: usize = 256;

pub(super) fn get(
    state: &AppState,
    principal: &Principal,
    query: &GetYardAccessQuery,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<YardAccessResponse>>, ApiError> {
    let yard = authorized_yard(state, principal, &query.yard_id)?;
    let now = now?;
    let visibility = effective_visibility(state, &yard.id)?;
    let grants = state
        .repository
        .list_yard_access_grants(&yard.id, now)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .map(grant_summary)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(success(YardAccessResponse {
        grants,
        visibility: api_visibility(visibility),
    }))
}

pub(super) fn set_visibility(
    state: &AppState,
    principal: &Principal,
    request: &SetYardVisibilityRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<YardVisibilityResponse>>, ApiError> {
    let yard = authorized_yard(state, principal, &request.yard_id)?;
    let now = now?;
    let visibility = domain_visibility(request.visibility);
    let previous = effective_visibility(state, &yard.id)?;
    let event = audit::event(
        yard.workspace_id.clone(),
        principal.0.id.clone(),
        "yard.visibility_changed",
        "yard_access_policy",
        vec![
            (
                "from".to_owned(),
                AuditValue::String(previous.as_str().to_owned()),
            ),
            (
                "to".to_owned(),
                AuditValue::String(visibility.as_str().to_owned()),
            ),
            ("yardId".to_owned(), AuditValue::String(yard.id.clone())),
        ],
        now,
    );
    let record = state
        .repository
        .set_yard_visibility(&yard.id, visibility, now, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(YardVisibilityResponse {
        visibility: api_visibility(record.visibility),
    }))
}

pub(super) fn grant(
    state: &AppState,
    principal: &Principal,
    request: &GrantYardAccessRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<YardAccessGrantResponse>>, ApiError> {
    let yard = authorized_yard(state, principal, &request.yard_id)?;
    let now = now?;
    validate_principal_id(&request.principal_id)?;
    if let Some(environment_id) = &request.environment_id {
        require_environment(state, &yard.id, environment_id)?;
    }
    let expires_at_ms = request
        .expires_at
        .as_deref()
        .map(|expires| parse_future_expiry(expires, now))
        .transpose()?;
    let grant = NewYardAccessGrant {
        id: format!("yardgrant_{}", uuid::Uuid::new_v4().simple()),
        yard_id: yard.id.clone(),
        environment_id: request.environment_id.clone(),
        principal_kind: domain_principal_kind(request.principal_kind),
        principal_id: request.principal_id.clone(),
        app_roles: request.app_roles.clone(),
        created_at_ms: now,
        created_by_principal: principal.0.id.clone(),
        expires_at_ms,
    };
    let event = audit::event(
        yard.workspace_id.clone(),
        principal.0.id.clone(),
        "yard.access_granted",
        "yard_access_grant",
        vec![
            (
                "environmentId".to_owned(),
                grant
                    .environment_id
                    .clone()
                    .map_or(AuditValue::Null, AuditValue::String),
            ),
            ("grantId".to_owned(), AuditValue::String(grant.id.clone())),
            (
                "principalKind".to_owned(),
                AuditValue::String(grant.principal_kind.as_str().to_owned()),
            ),
            ("yardId".to_owned(), AuditValue::String(yard.id)),
        ],
        now,
    );
    let record = state
        .repository
        .insert_yard_access_grant(&grant, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(YardAccessGrantResponse {
        grant: grant_summary(record)?,
    }))
}

pub(super) fn revoke(
    state: &AppState,
    principal: &Principal,
    request: &RevokeYardAccessRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let yard = authorized_yard(state, principal, &request.yard_id)?;
    let now = now?;
    let event = audit::event(
        yard.workspace_id.clone(),
        principal.0.id.clone(),
        "yard.access_revoked",
        "yard_access_grant",
        vec![
            (
                "grantId".to_owned(),
                AuditValue::String(request.grant_id.clone()),
            ),
            ("yardId".to_owned(), AuditValue::String(yard.id.clone())),
        ],
        now,
    );
    state
        .repository
        .revoke_yard_access_grant(&yard.id, &request.grant_id, now, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(EmptyResponse::default()))
}

fn authorized_yard(
    state: &AppState,
    principal: &Principal,
    yard_id: &str,
) -> Result<WebYardRecord, ApiError> {
    let yard = state
        .repository
        .web_yard_by_id(yard_id)
        .map_err(ApiError::from_repository)?;
    authorize_yard(principal, &yard)?;
    Ok(yard)
}

fn effective_visibility(state: &AppState, yard_id: &str) -> Result<YardVisibility, ApiError> {
    Ok(state
        .repository
        .get_yard_access_policy(yard_id)
        .map_err(ApiError::from_repository)?
        .map_or(YardVisibility::Public, |policy| policy.visibility))
}

fn require_environment(
    state: &AppState,
    yard_id: &str,
    environment_id: &str,
) -> Result<(), ApiError> {
    let known = state
        .repository
        .list_yard_environments(yard_id)
        .map_err(ApiError::from_repository)?
        .iter()
        .any(|environment| environment.id == environment_id);
    if known {
        Ok(())
    } else {
        Err(ApiError::invalid_request())
    }
}

fn validate_principal_id(value: &str) -> Result<(), ApiError> {
    let valid = !value.is_empty()
        && value.len() <= MAXIMUM_PRINCIPAL_LENGTH
        && value.trim() == value
        && !value.chars().any(char::is_control);
    if valid {
        Ok(())
    } else {
        Err(ApiError::invalid_request())
    }
}

fn parse_future_expiry(value: &str, now: u64) -> Result<u64, ApiError> {
    let parsed =
        OffsetDateTime::parse(value, &Rfc3339).map_err(|_error| ApiError::invalid_request())?;
    let milliseconds = u64::try_from(parsed.unix_timestamp_nanos() / 1_000_000)
        .map_err(|_error| ApiError::invalid_request())?;
    if milliseconds <= now {
        return Err(ApiError::invalid_request());
    }
    Ok(milliseconds)
}
