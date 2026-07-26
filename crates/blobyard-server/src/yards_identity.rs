use super::presentation::grant_summary;
use super::read::{yard_at, yard_by_id};
use crate::{
    api::AppState,
    audit,
    auth::Principal,
    error::ApiError,
    response::{Success, success},
};
use axum::Json;
use blobyard_api_client::{
    EmptyResponse, GetYardApplicationPolicyQuery, ListYardManagementRolesQuery,
    ListYardManagementRolesResponse, RevokeYardManagementRoleRequest, SetYardAccessRolesRequest,
    SetYardAccessRolesResponse, SetYardApplicationPolicyRequest, SetYardManagementRoleRequest,
    YardApplicationPolicyResponse, YardManagementRoleAssignment as ApiManagementRoleAssignment,
};
use blobyard_contract::{AuditValue, YardManagementRole, YardManagementRoleCursor};
use blobyard_core::canonicalize_application_policy;
use std::collections::BTreeSet;

#[path = "yards_identity_cursor.rs"]
mod cursor;
#[path = "yards_identity_presentation.rs"]
mod identity_presentation;

use identity_presentation::{api_assignment, api_policy, domain_role};

pub(super) fn list_management_roles(
    state: &AppState,
    principal: &Principal,
    query: &ListYardManagementRolesQuery,
) -> Result<Json<Success<ListYardManagementRolesResponse>>, ApiError> {
    let yard = yard_by_id(state, principal, &query.yard_id)?;
    let position = cursor::decode(&yard.id, query.cursor.as_deref())?;
    let page = state
        .repository
        .list_yard_management_roles(&yard.id, position.as_ref())
        .map_err(ApiError::from_repository)?;
    Ok(success(ListYardManagementRolesResponse {
        items: page
            .items
            .into_iter()
            .map(api_assignment)
            .collect::<Result<Vec<_>, _>>()?,
        next_cursor: page
            .next_cursor
            .as_ref()
            .map(|position| cursor::encode(&yard.id, position)),
    }))
}

pub(super) fn set_management_role(
    state: &AppState,
    principal: &Principal,
    request: &SetYardManagementRoleRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<ApiManagementRoleAssignment>>, ApiError> {
    let (yard, now) = yard_at(state, principal, &request.yard_id, now)?;
    let previous = role_for_user(state, &yard.id, &request.user_id)?;
    let role = domain_role(request.role);
    let event = audit::event(
        yard.workspace_id,
        principal.0.id.clone(),
        "yard.management_role_set",
        "yard_management_role",
        {
            let mut metadata = vec![
                (
                    "from".to_owned(),
                    previous.map_or(AuditValue::Null, |value| {
                        AuditValue::String(value.as_str().to_owned())
                    }),
                ),
                (
                    "to".to_owned(),
                    AuditValue::String(role.as_str().to_owned()),
                ),
            ];
            metadata.extend(role_target_metadata(&yard.id, &request.user_id));
            metadata
        },
        now,
    );
    let assignment = state
        .repository
        .set_yard_management_role(&yard.id, &request.user_id, role, now, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(api_assignment(assignment)?))
}

pub(super) fn revoke_management_role(
    state: &AppState,
    principal: &Principal,
    request: &RevokeYardManagementRoleRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<EmptyResponse>>, ApiError> {
    let (yard, now) = yard_at(state, principal, &request.yard_id, now)?;
    let previous =
        role_for_user(state, &yard.id, &request.user_id)?.ok_or_else(ApiError::not_found)?;
    let mut metadata = vec![(
        "from".to_owned(),
        AuditValue::String(previous.as_str().to_owned()),
    )];
    metadata.extend(role_target_metadata(&yard.id, &request.user_id));
    let event = audit::event(
        yard.workspace_id,
        principal.0.id.clone(),
        "yard.management_role_revoked",
        "yard_management_role",
        metadata,
        now,
    );
    state
        .repository
        .revoke_yard_management_role(&yard.id, &request.user_id, now, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(EmptyResponse::default()))
}

fn role_target_metadata(yard_id: &str, user_id: &str) -> [(String, AuditValue); 2] {
    [
        ("userId".to_owned(), AuditValue::String(user_id.to_owned())),
        ("yardId".to_owned(), AuditValue::String(yard_id.to_owned())),
    ]
}

pub(super) fn get_application_policy(
    state: &AppState,
    principal: &Principal,
    query: &GetYardApplicationPolicyQuery,
) -> Result<Json<Success<YardApplicationPolicyResponse>>, ApiError> {
    let yard = yard_by_id(state, principal, &query.yard_id)?;
    let policy = state
        .repository
        .get_yard_application_policy(&yard.id)
        .map_err(ApiError::from_repository)?
        .map(api_policy)
        .transpose()?;
    Ok(success(YardApplicationPolicyResponse { policy }))
}

pub(super) fn set_application_policy(
    state: &AppState,
    principal: &Principal,
    request: &SetYardApplicationPolicyRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<YardApplicationPolicyResponse>>, ApiError> {
    let (yard, now) = yard_at(state, principal, &request.yard_id, now)?;
    let canonical = canonicalize_application_policy(request.policy.clone())
        .map_err(|_error| ApiError::invalid_request())?;
    let previous = state
        .repository
        .get_yard_application_policy(&yard.id)
        .map_err(ApiError::from_repository)?;
    let from_revision = previous.as_ref().map(|record| record.revision);
    let to_revision = from_revision
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(ApiError::conflict)?;
    let event = audit::event(
        yard.workspace_id,
        principal.0.id.clone(),
        "yard.application_policy_set",
        "yard_application_policy",
        vec![
            (
                "fromRevision".to_owned(),
                from_revision.map_or(AuditValue::Null, AuditValue::Number),
            ),
            (
                "permissionCount".to_owned(),
                AuditValue::Number(application_permission_count(&canonical.graph)?),
            ),
            (
                "roleCount".to_owned(),
                AuditValue::Number(
                    u64::try_from(canonical.graph.roles.len())
                        .map_err(|_error| ApiError::internal())?,
                ),
            ),
            (
                "sourceManifestDigest".to_owned(),
                AuditValue::String(request.source_manifest_digest.clone()),
            ),
            ("toRevision".to_owned(), AuditValue::Number(to_revision)),
            ("yardId".to_owned(), AuditValue::String(yard.id.clone())),
        ],
        now,
    );
    let policy = state
        .repository
        .set_yard_application_policy(
            &yard.id,
            &request.source_manifest_digest,
            canonical.graph,
            &principal.0.id,
            now,
            &event,
        )
        .map_err(ApiError::from_repository)?;
    Ok(success(YardApplicationPolicyResponse {
        policy: Some(api_policy(policy)?),
    }))
}

fn application_permission_count(
    graph: &blobyard_core::ApplicationPolicyGraph,
) -> Result<u64, ApiError> {
    let count = graph
        .roles
        .values()
        .flat_map(|role| role.permissions.iter())
        .collect::<BTreeSet<_>>()
        .len();
    u64::try_from(count).map_err(|_error| ApiError::internal())
}

pub(super) fn set_access_roles(
    state: &AppState,
    principal: &Principal,
    request: &SetYardAccessRolesRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<SetYardAccessRolesResponse>>, ApiError> {
    let (yard, now) = yard_at(state, principal, &request.yard_id, now)?;
    let grant = state
        .repository
        .list_yard_access_grants(&yard.id, now)
        .map_err(ApiError::from_repository)?
        .into_iter()
        .find(|grant| grant.id == request.grant_id)
        .ok_or_else(ApiError::not_found)?;
    let mut to = request.app_roles.clone();
    to.sort();
    let mut from = grant.app_roles;
    from.sort();
    let event = audit::event(
        yard.workspace_id,
        principal.0.id.clone(),
        "yard.access_roles_set",
        "yard_access_grant",
        vec![
            (
                "from".to_owned(),
                AuditValue::String(identity_presentation::role_json(&from)?),
            ),
            (
                "grantId".to_owned(),
                AuditValue::String(request.grant_id.clone()),
            ),
            (
                "to".to_owned(),
                AuditValue::String(identity_presentation::role_json(&to)?),
            ),
            ("yardId".to_owned(), AuditValue::String(yard.id.clone())),
        ],
        now,
    );
    let grant = state
        .repository
        .set_yard_access_roles(&yard.id, &request.grant_id, &request.app_roles, now, &event)
        .map_err(ApiError::from_repository)?;
    Ok(success(SetYardAccessRolesResponse {
        grant: grant_summary(grant)?,
    }))
}

fn role_for_user(
    state: &AppState,
    yard_id: &str,
    user_id: &str,
) -> Result<Option<YardManagementRole>, ApiError> {
    let mut position: Option<YardManagementRoleCursor> = None;
    loop {
        let page = state
            .repository
            .list_yard_management_roles(yard_id, position.as_ref())
            .map_err(ApiError::from_repository)?;
        if let Some(assignment) = page.items.iter().find(|value| value.user_id == user_id) {
            return Ok(Some(assignment.role));
        }
        let Some(next) = page.next_cursor else {
            return Ok(None);
        };
        position = Some(next);
    }
}

#[cfg(test)]
#[path = "yards_identity_tests.rs"]
mod tests;
