use super::{Corruption, FaultingRepository};
use blobyard_contract::{
    NewAuditEvent, NewYardContinuation, NewYardSession, RepositoryError, YardAdmission,
    YardSessionAuditContext, YardSessionExchange, YardSessionListing, YardSessionRepository,
};

impl YardSessionRepository for FaultingRepository {
    fn evaluate_yard_admission(
        &self,
        host_label: &str,
        user_id: &str,
        now_ms: u64,
    ) -> Result<YardAdmission, RepositoryError> {
        self.check()?;
        self.inner
            .evaluate_yard_admission(host_label, user_id, now_ms)
    }

    fn issue_yard_exchange_code(
        &self,
        continuation: &NewYardContinuation,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.issue_yard_exchange_code(continuation)
    }

    fn exchange_yard_session_code(
        &self,
        code_hash: &str,
        host_label: &str,
        session: &NewYardSession,
        audit: &YardSessionAuditContext,
        now_ms: u64,
    ) -> Result<YardSessionExchange, RepositoryError> {
        self.check()?;
        self.inner
            .exchange_yard_session_code(code_hash, host_label, session, audit, now_ms)
    }

    fn list_yard_sessions(
        &self,
        yard_id: &str,
    ) -> Result<Vec<YardSessionListing>, RepositoryError> {
        self.check()?;
        self.inner.list_yard_sessions(yard_id).map(|mut listings| {
            if matches!(self.corruption, Some(Corruption::YardSessionCreatedAt))
                && let Some(listing) = listings.first_mut()
            {
                listing.session.created_at_ms = u64::MAX;
            }
            listings
        })
    }

    fn revoke_yard_session(
        &self,
        yard_id: &str,
        session_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        self.check()?;
        self.inner
            .revoke_yard_session(yard_id, session_id, now_ms, event)
    }

    fn revoke_yard_session_by_token(
        &self,
        token_hash: &str,
        host_label: &str,
        now_ms: u64,
    ) -> Result<bool, RepositoryError> {
        self.check()?;
        self.inner
            .revoke_yard_session_by_token(token_hash, host_label, now_ms)
    }

    fn purge_yard_session_history(&self, now_ms: u64) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.purge_yard_session_history(now_ms)
    }
}
