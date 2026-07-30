use super::Corrupting;
use super::Corruption;
use blobyard_contract::{
    NewYardOidcAttempt, NewYardOidcAuthentication, RepositoryError, YardOidcAttemptRecord,
    YardOidcAuditContext, YardOidcIdentityRecord, YardOidcRepository,
};

impl<T: YardOidcRepository> YardOidcRepository for Corrupting<'_, T> {
    fn create_yard_oidc_attempt(
        &self,
        attempt: &NewYardOidcAttempt,
    ) -> Result<(), RepositoryError> {
        self.inner.create_yard_oidc_attempt(attempt)
    }

    fn claim_yard_oidc_attempt(
        &self,
        state_hash: &str,
        now_ms: u64,
    ) -> Result<YardOidcAttemptRecord, RepositoryError> {
        self.inner.claim_yard_oidc_attempt(state_hash, now_ms)
    }

    fn authenticate_yard_oidc_identity(
        &self,
        authentication: &NewYardOidcAuthentication,
        audit: &YardOidcAuditContext,
    ) -> Result<YardOidcIdentityRecord, RepositoryError> {
        self.inner
            .authenticate_yard_oidc_identity(authentication, audit)
            .map(|mut identity| {
                match self.corruption {
                    Corruption::YardOidcMemberBinding
                        if authentication.provider_subject == "member-subject"
                            && authentication.authenticated_at_ms == 81 =>
                    {
                        "corrupt-subject".clone_into(&mut identity.yard_subject_id);
                    }
                    Corruption::YardOidcReturningBinding
                        if authentication.provider_subject == "member-subject"
                            && authentication.authenticated_at_ms == 82 =>
                    {
                        identity.last_authenticated_at_ms = 0;
                    }
                    Corruption::YardOidcGuestBinding
                        if authentication.provider_subject == "guest-subject" =>
                    {
                        "corrupt@example.test".clone_into(&mut identity.normalized_email);
                    }
                    _ => {}
                }
                identity
            })
    }
}
