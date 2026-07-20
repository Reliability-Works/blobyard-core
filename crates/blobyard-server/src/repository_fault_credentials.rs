use super::FaultingRepository;
use blobyard_contract::{
    CredentialRepository, LocalApiTokenRecord, LocalCliSessionRecord, NewAuditEvent,
    RepositoryError,
};

impl CredentialRepository for FaultingRepository {
    fn install_bootstrap(&self, hash: &str) -> Result<bool, RepositoryError> {
        self.check()?;
        self.inner.install_bootstrap(hash)
    }

    fn exchange_bootstrap(
        &self,
        hash: &str,
        token: &LocalApiTokenRecord,
        session: &LocalCliSessionRecord,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.exchange_bootstrap(hash, token, session)
    }

    fn list_cli_sessions(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<LocalCliSessionRecord>, RepositoryError> {
        self.check()?;
        self.inner.list_cli_sessions(workspace_id)
    }

    fn revoke_cli_session(
        &self,
        id: &str,
        workspace_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner
            .revoke_cli_session(id, workspace_id, now_ms, event)
    }

    fn create_api_token(
        &self,
        token: &LocalApiTokenRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.create_api_token(token, event)
    }

    fn list_api_tokens(&self) -> Result<Vec<LocalApiTokenRecord>, RepositoryError> {
        self.check()?;
        self.inner.list_api_tokens()
    }

    fn authenticate_api_token(
        &self,
        hash: &str,
        now_ms: u64,
    ) -> Result<LocalApiTokenRecord, RepositoryError> {
        self.check()?;
        self.inner.authenticate_api_token(hash, now_ms)
    }

    fn revoke_api_token(
        &self,
        id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.revoke_api_token(id, now_ms, event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{audit, auth, transfers::test_seams};

    #[test]
    fn credential_wrapper_forwards_bootstrap_and_session_lifecycle() {
        let fixture = test_seams::fixture(&["object:read", "tokens:manage"]);
        assert_eq!(
            FaultingRepository::new(fixture.state.repository.clone(), 0)
                .install_bootstrap(&auth::hash("unused")),
            Err(RepositoryError::Unavailable)
        );
        let repository = FaultingRepository::new(fixture.state.repository.clone(), usize::MAX);
        assert_eq!(
            repository.install_bootstrap(&auth::hash("unused")),
            Ok(false)
        );
        assert_eq!(
            FaultingRepository::new(fixture.state.repository.clone(), 0)
                .list_cli_sessions(&fixture.principal.workspace_id),
            Err(RepositoryError::Unavailable)
        );
        let session_event =
            audit::cli_session_revoked_event(&fixture.principal, "session_fixture", 2);
        assert_eq!(
            FaultingRepository::new(fixture.state.repository.clone(), 0).revoke_cli_session(
                "session_fixture",
                &fixture.principal.workspace_id,
                2,
                &session_event,
            ),
            Err(RepositoryError::Unavailable)
        );
        assert_eq!(
            repository
                .list_cli_sessions(&fixture.principal.workspace_id)
                .expect("forwarded session list")
                .len(),
            1
        );
        repository
            .revoke_cli_session(
                "session_fixture",
                &fixture.principal.workspace_id,
                2,
                &session_event,
            )
            .expect("forwarded session revocation");
    }

    #[test]
    fn credential_wrapper_forwards_token_creation_listing_and_revocation() {
        let fixture = test_seams::fixture(&["object:read", "tokens:manage"]);
        let repository = FaultingRepository::new(fixture.state.repository.clone(), usize::MAX);
        let token = LocalApiTokenRecord {
            id: "token_forwarded".to_owned(),
            name: "Forwarded".to_owned(),
            token_prefix: "byd_pat_forward".to_owned(),
            secret_hash: auth::hash("forwarded"),
            scopes: vec!["object:read".to_owned()],
            workspace_id: fixture.principal.workspace_id.clone(),
            project_id: None,
            created_at_ms: 10,
            expires_at_ms: 100,
            last_used_at_ms: None,
            revoked_at_ms: None,
        };
        let created = audit::api_token_created_event(&fixture.principal, &token);
        repository
            .create_api_token(&token, &created)
            .expect("forwarded creation");
        assert!(
            repository
                .list_api_tokens()
                .expect("forwarded list")
                .iter()
                .any(|candidate| candidate.id == token.id)
        );
        let revoked = audit::api_token_revoked_event(&fixture.principal, &token.id, 11);
        repository
            .revoke_api_token(&token.id, 11, &revoked)
            .expect("forwarded revocation");
    }
}
