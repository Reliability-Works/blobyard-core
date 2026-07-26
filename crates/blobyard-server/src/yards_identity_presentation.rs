use crate::error::ApiError;
use blobyard_api_client::{
    YardApplicationPolicy, YardManagementRole as ApiManagementRole,
    YardManagementRoleAssignment as ApiManagementRoleAssignment,
};
use blobyard_contract::{
    YardApplicationPolicyRecord, YardManagementRole, YardManagementRoleAssignment,
};

pub(super) fn api_assignment(
    assignment: YardManagementRoleAssignment,
) -> Result<ApiManagementRoleAssignment, ApiError> {
    Ok(ApiManagementRoleAssignment {
        user_id: assignment.user_id,
        role: api_role(assignment.role),
        created_at: crate::transfer_grants::format_expiry(assignment.created_at_ms)?,
        updated_at: crate::transfer_grants::format_expiry(assignment.updated_at_ms)?,
    })
}

pub(super) fn api_policy(
    record: YardApplicationPolicyRecord,
) -> Result<YardApplicationPolicy, ApiError> {
    Ok(YardApplicationPolicy {
        revision: record.revision,
        source_manifest_digest: record.source_manifest_digest,
        graph: record.policy,
        approved_at: crate::transfer_grants::format_expiry(record.approved_at_ms)?,
        approved_by_principal_id: record.approved_by_principal,
    })
}

pub(super) const fn domain_role(role: ApiManagementRole) -> YardManagementRole {
    match role {
        ApiManagementRole::Owner => YardManagementRole::Owner,
        ApiManagementRole::Admin => YardManagementRole::Admin,
        ApiManagementRole::Developer => YardManagementRole::Developer,
        ApiManagementRole::Auditor => YardManagementRole::Auditor,
    }
}

pub(super) const fn api_role(role: YardManagementRole) -> ApiManagementRole {
    match role {
        YardManagementRole::Owner => ApiManagementRole::Owner,
        YardManagementRole::Admin => ApiManagementRole::Admin,
        YardManagementRole::Developer => ApiManagementRole::Developer,
        YardManagementRole::Auditor => ApiManagementRole::Auditor,
    }
}

pub(super) fn role_json(roles: &[String]) -> Result<String, ApiError> {
    serde_json::to_string(roles).map_err(|_error| ApiError::internal())
}
