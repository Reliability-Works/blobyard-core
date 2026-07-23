use super::FaultingRepository;
use blobyard_contract::{
    LocalUserListing, LocalUserLoginKeyRecord, LocalUserRecord, LocalUserRepository, NewAuditEvent,
    RepositoryError,
};

impl LocalUserRepository for FaultingRepository {
    fn create_local_user(
        &self,
        user: &LocalUserRecord,
        key: &LocalUserLoginKeyRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.create_local_user(user, key, event)
    }

    fn list_local_users(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<LocalUserListing>, RepositoryError> {
        self.check()?;
        self.inner.list_local_users(workspace_id)
    }

    fn reset_local_user_login_key(
        &self,
        key: &LocalUserLoginKeyRecord,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.reset_local_user_login_key(key, now_ms, event)
    }

    fn deactivate_local_user(
        &self,
        user_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.deactivate_local_user(user_id, now_ms, event)
    }

    fn authenticate_local_user_key(
        &self,
        secret_hash: &str,
        now_ms: u64,
    ) -> Result<LocalUserRecord, RepositoryError> {
        self.check()?;
        self.inner.authenticate_local_user_key(secret_hash, now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfers::test_seams;
    use blobyard_testkit::{local_user, local_user_event, login_key};

    #[test]
    fn local_user_wrapper_forwards_each_lifecycle_operation() {
        let fixture = test_seams::fixture(&["object:read"]);
        let workspace_id = fixture.principal.workspace_id.as_str();
        let user = local_user(workspace_id, "user_forwarded", None, 10);
        let key = login_key("userkey_forwarded", "user_forwarded", 'c', 10);
        let created = local_user_event("audit_user_forwarded", &user, "user.created", 10);
        let faulted = FaultingRepository::new(fixture.state.repository.clone(), 0);
        assert_eq!(
            faulted.create_local_user(&user, &key, &created),
            Err(RepositoryError::Unavailable)
        );
        assert_eq!(
            faulted.authenticate_local_user_key(&key.secret_hash, 11),
            Err(RepositoryError::Unavailable)
        );
        assert_eq!(
            faulted.reset_local_user_login_key(&key, 10, &created),
            Err(RepositoryError::Unavailable)
        );
        assert_eq!(
            faulted.deactivate_local_user(&user.id, 10, &created),
            Err(RepositoryError::Unavailable)
        );
        let repository = FaultingRepository::new(fixture.state.repository.clone(), usize::MAX);
        repository
            .create_local_user(&user, &key, &created)
            .expect("forwarded creation");
        assert_eq!(
            FaultingRepository::new(fixture.state.repository.clone(), 0)
                .list_local_users(workspace_id),
            Err(RepositoryError::Unavailable)
        );
        assert_eq!(
            repository
                .list_local_users(workspace_id)
                .expect("forwarded list")
                .len(),
            1
        );
        assert_eq!(
            repository
                .authenticate_local_user_key(&key.secret_hash, 11)
                .expect("forwarded authentication")
                .id,
            user.id
        );
        let replacement = login_key("userkey_replacement", "user_forwarded", 'd', 12);
        let reset = local_user_event("audit_key_forwarded", &user, "user.login_key_reset", 12);
        repository
            .reset_local_user_login_key(&replacement, 12, &reset)
            .expect("forwarded reset");
        let deactivated = local_user_event("audit_user_gone", &user, "user.deactivated", 13);
        repository
            .deactivate_local_user(&user.id, 13, &deactivated)
            .expect("forwarded deactivation");
    }
}
