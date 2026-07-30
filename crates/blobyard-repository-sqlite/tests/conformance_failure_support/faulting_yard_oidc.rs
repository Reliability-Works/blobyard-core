use super::Faulting;
use blobyard_contract::{
    NewYardOidcAttempt, NewYardOidcAuthentication, RepositoryError, YardOidcAttemptRecord,
    YardOidcAuditContext, YardOidcIdentityRecord, YardOidcRepository,
};

impl<T: YardOidcRepository> YardOidcRepository for Faulting<'_, T> {
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
