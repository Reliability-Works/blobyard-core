use super::Faulting;
use blobyard_contract::{
    NewAuditEvent, RepositoryError, YardAccessGrantRecord, YardApplicationPolicyRecord,
    YardIdentity, YardIdentityRepository, YardManagementRole, YardManagementRoleAssignment,
    YardManagementRoleCursor, YardManagementRolePage,
};
use blobyard_core::ApplicationPolicyGraph;

impl<T: YardIdentityRepository> YardIdentityRepository for Faulting<'_, T> {
    fn list_yard_management_roles(
        &self,
        yard_id: &str,
        cursor: Option<&YardManagementRoleCursor>,
    ) -> Result<YardManagementRolePage, RepositoryError> {
        self.check()?;
        self.inner.list_yard_management_roles(yard_id, cursor)
    }

    fn set_yard_management_role(
        &self,
        yard_id: &str,
        user_id: &str,
        role: YardManagementRole,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardManagementRoleAssignment, RepositoryError> {
        self.check()?;
        self.inner
            .set_yard_management_role(yard_id, user_id, role, now_ms, event)
    }

    fn revoke_yard_management_role(
        &self,
        yard_id: &str,
        user_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner
            .revoke_yard_management_role(yard_id, user_id, now_ms, event)
    }

    fn get_yard_application_policy(
        &self,
        yard_id: &str,
    ) -> Result<Option<YardApplicationPolicyRecord>, RepositoryError> {
        self.check()?;
        self.inner.get_yard_application_policy(yard_id)
    }

    fn set_yard_application_policy(
        &self,
        yard_id: &str,
        source_manifest_digest: &str,
        policy: ApplicationPolicyGraph,
        approved_by_principal: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardApplicationPolicyRecord, RepositoryError> {
        self.check()?;
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
        event: &NewAuditEvent,
    ) -> Result<YardAccessGrantRecord, RepositoryError> {
        self.check()?;
        self.inner
            .set_yard_access_roles(yard_id, grant_id, app_roles, now_ms, event)
    }

    fn resolve_yard_identity(
        &self,
        host_label: &str,
        session_token_hash: &str,
        now_ms: u64,
    ) -> Result<YardIdentity, RepositoryError> {
        self.check()?;
        self.inner
            .resolve_yard_identity(host_label, session_token_hash, now_ms)
    }
}
