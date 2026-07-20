use super::{Corrupting, Corruption};
use blobyard_contract::{
    CredentialRepository, LocalApiTokenRecord, LocalCliSessionRecord, NewAuditEvent,
    RepositoryError,
};

impl<T: CredentialRepository> CredentialRepository for Corrupting<'_, T> {
    fn install_bootstrap(&self, hash: &str) -> Result<bool, RepositoryError> {
        self.inner
            .install_bootstrap(hash)
            .map(|value| match self.corruption {
                Corruption::BootstrapFirstFalse if hash.starts_with('b') => false,
                Corruption::BootstrapSecondTrue if hash.starts_with('c') => true,
                _ => value,
            })
    }

    fn exchange_bootstrap(
        &self,
        hash: &str,
        token: &LocalApiTokenRecord,
        session: &LocalCliSessionRecord,
    ) -> Result<(), RepositoryError> {
        self.inner.exchange_bootstrap(hash, token, session)
    }

    fn list_cli_sessions(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<LocalCliSessionRecord>, RepositoryError> {
        self.inner
            .list_cli_sessions(workspace_id)
            .map(|mut values| {
                if matches!(self.corruption, Corruption::CliSessionList) {
                    values.clear();
                }
                values
            })
    }

    fn revoke_cli_session(
        &self,
        id: &str,
        workspace_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.inner
            .revoke_cli_session(id, workspace_id, now_ms, event)
    }

    fn create_api_token(
        &self,
        token: &LocalApiTokenRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.inner.create_api_token(token, event)
    }

    fn list_api_tokens(&self) -> Result<Vec<LocalApiTokenRecord>, RepositoryError> {
        self.inner.list_api_tokens().and_then(|mut values| {
            if matches!(self.corruption, Corruption::FinalTokenListError)
                && values.iter().any(|token| token.revoked_at_ms.is_some())
            {
                return Err(RepositoryError::Unavailable);
            }
            if matches!(self.corruption, Corruption::FinalTokenListMismatch)
                && values.iter().any(|token| token.revoked_at_ms.is_some())
            {
                values.clear();
            }
            if matches!(self.corruption, Corruption::TokenList) {
                values.pop();
            }
            Ok(values)
        })
    }

    fn authenticate_api_token(
        &self,
        hash: &str,
        now_ms: u64,
    ) -> Result<LocalApiTokenRecord, RepositoryError> {
        self.inner
            .authenticate_api_token(hash, now_ms)
            .map(|mut value| {
                let corrupt = matches!(self.corruption, Corruption::ActiveTokenRecord)
                    || matches!(self.corruption, Corruption::CreatedTokenRecord)
                        && hash.starts_with('e')
                        && now_ms == 6
                    || matches!(self.corruption, Corruption::MonotonicTokenRecord)
                        && hash.starts_with('e')
                        && now_ms == 5;
                if corrupt {
                    value.name.push_str(" changed");
                }
                value
            })
    }

    fn revoke_api_token(
        &self,
        id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.inner.revoke_api_token(id, now_ms, event)
    }
}
