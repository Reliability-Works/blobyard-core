use super::{
    SqliteRepository, map_error, rows, transfer_validation, yard_session_admission,
    yard_session_rows, yard_session_store,
};
use blobyard_contract::{
    NewAuditEvent, NewYardContinuation, NewYardSession, RepositoryError, YardAdmission,
    YardSessionAuditContext, YardSessionExchange, YardSessionListing, YardSessionRepository,
};

impl YardSessionRepository for SqliteRepository {
    fn evaluate_yard_admission(
        &self,
        host_label: &str,
        user_id: &str,
        now_ms: u64,
    ) -> Result<YardAdmission, RepositoryError> {
        let now = transfer_validation::to_i64(now_ms)?;
        self.connection().and_then(|connection| {
            yard_session_admission::evaluate(&connection, host_label, user_id, now)
        })
    }

    fn issue_yard_exchange_code(
        &self,
        continuation: &NewYardContinuation,
    ) -> Result<(), RepositoryError> {
        self.write_transaction(|transaction| yard_session_store::issue(transaction, continuation))
    }

    fn exchange_yard_session_code(
        &self,
        code_hash: &str,
        host_label: &str,
        session: &NewYardSession,
        audit: &YardSessionAuditContext,
        now_ms: u64,
    ) -> Result<YardSessionExchange, RepositoryError> {
        self.write_transaction(|transaction| {
            yard_session_store::exchange(transaction, code_hash, host_label, session, audit, now_ms)
        })
    }

    fn list_yard_sessions(
        &self,
        yard_id: &str,
    ) -> Result<Vec<YardSessionListing>, RepositoryError> {
        rows::validate_text(yard_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {}, u.display_name FROM yard_sessions s JOIN local_users u ON u.id = s.user_id WHERE s.yard_id = ?1 ORDER BY s.created_at_ms DESC, s.id DESC",
                qualified_session_columns()
            ))
            .map_err(map_error)?;
        let result = yard_session_store::list(&mut statement, yard_id);
        drop(statement);
        drop(connection);
        result
    }

    fn revoke_yard_session(
        &self,
        yard_id: &str,
        session_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        rows::validate_text(yard_id)?;
        rows::validate_text(session_id)?;
        self.write_transaction(|transaction| {
            yard_session_store::revoke(transaction, yard_id, session_id, now_ms, event)
        })
    }

    fn revoke_yard_session_by_token(
        &self,
        token_hash: &str,
        host_label: &str,
        now_ms: u64,
    ) -> Result<bool, RepositoryError> {
        self.write_transaction(|transaction| {
            yard_session_store::revoke_by_token(transaction, token_hash, host_label, now_ms)
        })
    }

    fn purge_yard_session_history(&self, now_ms: u64) -> Result<(), RepositoryError> {
        self.write_transaction(|transaction| yard_session_store::purge(transaction, now_ms))
    }
}

fn qualified_session_columns() -> String {
    yard_session_rows::SESSION_COLUMNS
        .split(", ")
        .map(|column| format!("s.{column}"))
        .collect::<Vec<_>>()
        .join(", ")
}
