use super::{
    SqliteRepository, transfer_validation, yard_application_policy, yard_identity_resolution,
    yard_management_roles,
};
use blobyard_contract::{
    NewAuditEvent, RepositoryError, YardAccessGrantRecord, YardApplicationPolicyRecord,
    YardIdentity, YardIdentityRepository, YardManagementRole, YardManagementRoleAssignment,
    YardManagementRoleCursor, YardManagementRolePage,
};
use blobyard_core::ApplicationPolicyGraph;

impl YardIdentityRepository for SqliteRepository {
    fn list_yard_management_roles(
        &self,
        yard_id: &str,
        cursor: Option<&YardManagementRoleCursor>,
    ) -> Result<YardManagementRolePage, RepositoryError> {
        let connection = self.connection()?;
        yard_management_roles::list(&connection, yard_id, cursor)
    }

    fn set_yard_management_role(
        &self,
        yard_id: &str,
        user_id: &str,
        role: YardManagementRole,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardManagementRoleAssignment, RepositoryError> {
        let now = transfer_validation::to_i64(now_ms)?;
        self.write_transaction(|transaction| {
            yard_management_roles::set(transaction, yard_id, user_id, role, now, event)
        })
    }

    fn revoke_yard_management_role(
        &self,
        yard_id: &str,
        user_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        let now = transfer_validation::to_i64(now_ms)?;
        self.write_transaction(|transaction| {
            yard_management_roles::revoke(transaction, yard_id, user_id, now, event)
        })
    }

    fn get_yard_application_policy(
        &self,
        yard_id: &str,
    ) -> Result<Option<YardApplicationPolicyRecord>, RepositoryError> {
        let connection = self.connection()?;
        yard_application_policy::get(&connection, yard_id)
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
        let now = transfer_validation::to_i64(now_ms)?;
        self.write_transaction(|transaction| {
            yard_application_policy::set(
                transaction,
                yard_id,
                source_manifest_digest,
                policy,
                approved_by_principal,
                now,
                event,
            )
        })
    }

    fn set_yard_access_roles(
        &self,
        yard_id: &str,
        grant_id: &str,
        app_roles: &[String],
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardAccessGrantRecord, RepositoryError> {
        let now = transfer_validation::to_i64(now_ms)?;
        self.write_transaction(|transaction| {
            yard_application_policy::set_grant_roles(
                transaction,
                yard_id,
                grant_id,
                app_roles,
                now,
                event,
            )
        })
    }

    fn resolve_yard_identity(
        &self,
        host_label: &str,
        session_token_hash: &str,
        now_ms: u64,
    ) -> Result<YardIdentity, RepositoryError> {
        let now = transfer_validation::to_i64(now_ms)?;
        let connection = self.connection()?;
        yard_identity_resolution::resolve(&connection, host_label, session_token_hash, now)
    }
}
