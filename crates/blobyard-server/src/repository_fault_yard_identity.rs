use super::{Corruption, FaultingRepository};
use blobyard_contract::{
    NewAuditEvent, RepositoryError, YardAccessGrantRecord, YardApplicationPolicyRecord,
    YardIdentity, YardIdentityRepository, YardManagementRole, YardManagementRoleAssignment,
    YardManagementRoleCursor, YardManagementRolePage,
};
use blobyard_core::ApplicationPolicyGraph;

impl YardIdentityRepository for FaultingRepository {
    fn list_yard_management_roles(
        &self,
        yard_id: &str,
        cursor: Option<&YardManagementRoleCursor>,
    ) -> Result<YardManagementRolePage, RepositoryError> {
        self.check()?;
        let mut page = self.inner.list_yard_management_roles(yard_id, cursor)?;
        if matches!(
            self.corruption,
            Some(Corruption::YardManagementRoleTimestamp)
        ) && let Some(assignment) = page.items.first_mut()
        {
            assignment.updated_at_ms = u64::MAX;
        }
        Ok(page)
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
        let mut assignment = self
            .inner
            .set_yard_management_role(yard_id, user_id, role, now_ms, event)?;
        if matches!(
            self.corruption,
            Some(Corruption::YardManagementRoleTimestamp)
        ) {
            assignment.updated_at_ms = u64::MAX;
        }
        Ok(assignment)
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
        let mut policy = self.inner.get_yard_application_policy(yard_id)?;
        if let Some(policy) = policy.as_mut() {
            match self.corruption {
                Some(Corruption::YardPolicyTimestamp) => policy.approved_at_ms = u64::MAX,
                Some(Corruption::YardPolicyRevision) => policy.revision = u64::MAX,
                Some(
                    Corruption::CompletedVersion
                    | Corruption::CompletedPath
                    | Corruption::CompletedSize
                    | Corruption::CompletedChecksum
                    | Corruption::AbortedStorageKey
                    | Corruption::ShareObjectSize
                    | Corruption::ShareExpiry
                    | Corruption::InboxExpiry
                    | Corruption::PreviewCreatedAt
                    | Corruption::PreviewExpiresAt
                    | Corruption::YardSessionCreatedAt
                    | Corruption::YardManagementRoleTimestamp
                    | Corruption::YardAccessGrantTimestamp
                    | Corruption::YardGuestInviteTimestamp,
                )
                | None => {}
            }
        }
        Ok(policy)
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
        let mut policy = self.inner.set_yard_application_policy(
            yard_id,
            source_manifest_digest,
            policy,
            approved_by_principal,
            now_ms,
            event,
        )?;
        if matches!(self.corruption, Some(Corruption::YardPolicyTimestamp)) {
            policy.approved_at_ms = u64::MAX;
        }
        Ok(policy)
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
        let mut grant = self
            .inner
            .set_yard_access_roles(yard_id, grant_id, app_roles, now_ms, event)?;
        if matches!(self.corruption, Some(Corruption::YardAccessGrantTimestamp)) {
            grant.created_at_ms = u64::MAX;
        }
        Ok(grant)
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
