use super::{Corrupting, Corruption};

impl<T: blobyard_contract::YardIdentityRepository> blobyard_contract::YardIdentityRepository
    for Corrupting<'_, T>
{
    fn list_yard_management_roles(
        &self,
        yard_id: &str,
        cursor: Option<&blobyard_contract::YardManagementRoleCursor>,
    ) -> Result<blobyard_contract::YardManagementRolePage, blobyard_contract::RepositoryError> {
        self.inner.list_yard_management_roles(yard_id, cursor)
    }

    fn set_yard_management_role(
        &self,
        yard_id: &str,
        user_id: &str,
        role: blobyard_contract::YardManagementRole,
        now_ms: u64,
        event: &blobyard_contract::NewAuditEvent,
    ) -> Result<blobyard_contract::YardManagementRoleAssignment, blobyard_contract::RepositoryError>
    {
        self.inner
            .set_yard_management_role(yard_id, user_id, role, now_ms, event)
    }

    fn revoke_yard_management_role(
        &self,
        yard_id: &str,
        user_id: &str,
        now_ms: u64,
        event: &blobyard_contract::NewAuditEvent,
    ) -> Result<(), blobyard_contract::RepositoryError> {
        self.inner
            .revoke_yard_management_role(yard_id, user_id, now_ms, event)
    }

    fn get_yard_application_policy(
        &self,
        yard_id: &str,
    ) -> Result<
        Option<blobyard_contract::YardApplicationPolicyRecord>,
        blobyard_contract::RepositoryError,
    > {
        self.inner.get_yard_application_policy(yard_id)
    }

    fn set_yard_application_policy(
        &self,
        yard_id: &str,
        source_manifest_digest: &str,
        policy: blobyard_core::ApplicationPolicyGraph,
        approved_by_principal: &str,
        now_ms: u64,
        event: &blobyard_contract::NewAuditEvent,
    ) -> Result<blobyard_contract::YardApplicationPolicyRecord, blobyard_contract::RepositoryError>
    {
        self.inner.set_yard_application_policy(
            yard_id,
            source_manifest_digest,
            policy,
            approved_by_principal,
            now_ms,
            event,
        )
    }

    fn set_yard_access_roles(
        &self,
        yard_id: &str,
        grant_id: &str,
        app_roles: &[String],
        now_ms: u64,
        event: &blobyard_contract::NewAuditEvent,
    ) -> Result<blobyard_contract::YardAccessGrantRecord, blobyard_contract::RepositoryError> {
        self.inner
            .set_yard_access_roles(yard_id, grant_id, app_roles, now_ms, event)
    }

    fn resolve_yard_identity(
        &self,
        host_label: &str,
        session_token_hash: &str,
        now_ms: u64,
    ) -> Result<blobyard_contract::YardIdentity, blobyard_contract::RepositoryError> {
        self.inner
            .resolve_yard_identity(host_label, session_token_hash, now_ms)
            .map(|mut identity| {
                if matches!(self.corruption, Corruption::YardGuestIdentityRecord)
                    && identity.user_id.starts_with("guest_")
                {
                    identity.app_roles.clear();
                }
                identity
            })
    }
}
