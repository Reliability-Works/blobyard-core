use super::Faulting;
use blobyard_contract::{
    LocalUserListing, LocalUserLoginKeyRecord, LocalUserRecord, LocalUserRepository, NewAuditEvent,
    RepositoryError,
};

impl<T: LocalUserRepository> LocalUserRepository for Faulting<'_, T> {
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
