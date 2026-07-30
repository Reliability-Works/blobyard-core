use super::{
    SqliteRepository, auth_validation, changed_once, collect, lifecycle_audit, map_error,
    rows as repository_rows, yard_guest_rows, yard_rows, yard_session_rows, yard_session_store,
};
use blobyard_contract::{
    NewYardOidcAttempt, NewYardOidcAuthentication, RepositoryError, YardOidcAttemptRecord,
    YardOidcAuditContext, YardOidcIdentityRecord, YardOidcRepository,
};

mod attempts;
mod authority;
mod identities;
mod rows;
mod validation;

impl YardOidcRepository for SqliteRepository {
    fn create_yard_oidc_attempt(
        &self,
        attempt: &NewYardOidcAttempt,
    ) -> Result<(), RepositoryError> {
        self.write_transaction(|transaction| attempts::create(transaction, attempt))
    }

    fn claim_yard_oidc_attempt(
        &self,
        state_hash: &str,
        now_ms: u64,
    ) -> Result<YardOidcAttemptRecord, RepositoryError> {
        self.write_transaction(|transaction| attempts::claim(transaction, state_hash, now_ms))
    }

    fn authenticate_yard_oidc_identity(
        &self,
        authentication: &NewYardOidcAuthentication,
        audit: &YardOidcAuditContext,
    ) -> Result<YardOidcIdentityRecord, RepositoryError> {
        let outcome = self.write_transaction(|transaction| {
            identities::authenticate(transaction, authentication, audit)
        })?;
        match outcome {
            identities::AuthenticationOutcome::Authenticated(identity) => Ok(identity),
            identities::AuthenticationOutcome::Denied => Err(RepositoryError::NotFound),
        }
    }
}
