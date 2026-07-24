use super::Corrupting;
use super::Corruption;
use blobyard_contract::{
    NewAuditEvent, NewYardContinuation, NewYardSession, RepositoryError, YardAdmission,
    YardSessionAuditContext, YardSessionExchange, YardSessionListing, YardSessionRepository,
};

impl<T: YardSessionRepository> YardSessionRepository for Corrupting<'_, T> {
    fn evaluate_yard_admission(
        &self,
        host_label: &str,
        user_id: &str,
        now_ms: u64,
    ) -> Result<YardAdmission, RepositoryError> {
        self.inner
            .evaluate_yard_admission(host_label, user_id, now_ms)
            .map(|mut admission| {
                if matches!(self.corruption, Corruption::YardSessionAdmission) {
                    admission.yard_id.push_str("_corrupt");
                }
                admission
            })
    }

    fn issue_yard_exchange_code(
        &self,
        continuation: &NewYardContinuation,
    ) -> Result<(), RepositoryError> {
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
        self.inner
            .exchange_yard_session_code(code_hash, host_label, session, audit, now_ms)
            .map(|mut exchange| {
                if matches!(self.corruption, Corruption::YardSessionExchange) {
                    exchange.return_path.push_str("_corrupt");
                }
                exchange
            })
    }

    fn list_yard_sessions(
        &self,
        yard_id: &str,
    ) -> Result<Vec<YardSessionListing>, RepositoryError> {
        self.inner.list_yard_sessions(yard_id).map(|mut listings| {
            if matches!(self.corruption, Corruption::YardSessionList)
                && let Some(listing) = listings.first_mut()
            {
                listing.user_display_name.push_str("_corrupt");
            } else if matches!(self.corruption, Corruption::YardSessionDeactivation)
                && let Some(listing) = listings
                    .iter_mut()
                    .find(|listing| listing.session.id == "yardsession_deactivated")
            {
                listing.session.revoked_at_ms = None;
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
        self.inner
            .revoke_yard_session(yard_id, session_id, now_ms, event)
            .map(|revoked| {
                if matches!(self.corruption, Corruption::YardSessionFirstRevoke) && now_ms == 140 {
                    !revoked
                } else {
                    revoked
                }
            })
    }

    fn revoke_yard_session_by_token(
        &self,
        token_hash: &str,
        host_label: &str,
        now_ms: u64,
    ) -> Result<bool, RepositoryError> {
        self.inner
            .revoke_yard_session_by_token(token_hash, host_label, now_ms)
            .map(|revoked| {
                if matches!(self.corruption, Corruption::YardSessionLogoutRevoke) && now_ms == 151 {
                    !revoked
                } else {
                    revoked
                }
            })
    }

    fn purge_yard_session_history(&self, now_ms: u64) -> Result<(), RepositoryError> {
        self.inner.purge_yard_session_history(now_ms)
    }
}
