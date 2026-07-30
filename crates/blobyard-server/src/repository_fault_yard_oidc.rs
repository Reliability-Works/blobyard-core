use super::FaultingRepository;
use blobyard_contract::{
    NewYardOidcAttempt, NewYardOidcAuthentication, RepositoryError, YardOidcAttemptRecord,
    YardOidcAuditContext, YardOidcIdentityRecord, YardOidcRepository,
};

impl YardOidcRepository for FaultingRepository {
    fn create_yard_oidc_attempt(
        &self,
        attempt: &NewYardOidcAttempt,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.create_yard_oidc_attempt(attempt)
    }

    fn claim_yard_oidc_attempt(
        &self,
        state_hash: &str,
        now_ms: u64,
    ) -> Result<YardOidcAttemptRecord, RepositoryError> {
        self.check()?;
        self.inner.claim_yard_oidc_attempt(state_hash, now_ms)
    }

    fn authenticate_yard_oidc_identity(
        &self,
        authentication: &NewYardOidcAuthentication,
        audit: &YardOidcAuditContext,
    ) -> Result<YardOidcIdentityRecord, RepositoryError> {
        self.check()?;
        self.inner
            .authenticate_yard_oidc_identity(authentication, audit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Repository;
    use std::sync::Arc;

    #[test]
    fn attempt_creation_fails_at_the_repository_seam() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let inner: Arc<dyn Repository> = Arc::new(
            blobyard_repository_sqlite::SqliteRepository::open(
                &temporary.path().join("metadata.sqlite3"),
            )
            .expect("repository"),
        );
        let attempt = NewYardOidcAttempt {
            state_hash: "a".repeat(64),
            continuation_hash: "b".repeat(64),
            host_label: "yard-123456789-fixture".to_owned(),
            return_path: "/".to_owned(),
            created_at_ms: 1,
            expires_at_ms: 600_001,
        };

        assert_eq!(
            FaultingRepository::new(inner, 0).create_yard_oidc_attempt(&attempt),
            Err(RepositoryError::Unavailable)
        );
    }
}
