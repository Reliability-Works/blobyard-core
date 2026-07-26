use super::{Corrupting, Corruption};
use blobyard_contract::{
    LocalUserListing, LocalUserLoginKeyRecord, LocalUserRecord, LocalUserRepository, NewAuditEvent,
    RepositoryError,
};

impl<T: LocalUserRepository> LocalUserRepository for Corrupting<'_, T> {
    fn create_local_user(
        &self,
        user: &LocalUserRecord,
        key: &LocalUserLoginKeyRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.inner.create_local_user(user, key, event)
    }

    fn list_local_users(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<LocalUserListing>, RepositoryError> {
        let mut records = self.inner.list_local_users(workspace_id)?;
        if matches!(self.corruption, Corruption::LocalUserInitialList) && records.is_empty() {
            records.push(LocalUserListing {
                user: blobyard_testkit::local_user(workspace_id, "user_unexpected", None, 1),
                active_key_prefix: None,
            });
        }
        if matches!(self.corruption, Corruption::GroupMissingUser) {
            records.retain(|listing| listing.user.id != "user_first");
        }
        Ok(records)
    }

    fn reset_local_user_login_key(
        &self,
        key: &LocalUserLoginKeyRecord,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.inner.reset_local_user_login_key(key, now_ms, event)
    }

    fn deactivate_local_user(
        &self,
        user_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.inner.deactivate_local_user(user_id, now_ms, event)
    }

    fn authenticate_local_user_key(
        &self,
        secret_hash: &str,
        now_ms: u64,
    ) -> Result<LocalUserRecord, RepositoryError> {
        self.inner
            .authenticate_local_user_key(secret_hash, now_ms)
            .map(|mut user| {
                let corrupt = matches!(
                    (self.corruption, now_ms),
                    (Corruption::LocalUserFreshAuthentication, 21)
                        | (Corruption::LocalUserBoundaryAuthentication, 20)
                        | (Corruption::LocalUserResetAuthentication, 31)
                );
                if corrupt {
                    user.id.push_str("_corrupt");
                }
                user
            })
    }
}
