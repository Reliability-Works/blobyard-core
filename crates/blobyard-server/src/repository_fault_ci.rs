use super::FaultingRepository;
use blobyard_contract::{
    CiRepository, LocalCiTrustRecord, LocalMachineSessionRecord, MachineSessionMintResult,
    NewAuditEvent, NewMachineSession, RepositoryError,
};

impl CiRepository for FaultingRepository {
    fn create_ci_trust(
        &self,
        trust: &LocalCiTrustRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.create_ci_trust(trust, event)
    }

    fn list_ci_trusts(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<LocalCiTrustRecord>, RepositoryError> {
        self.check()?;
        self.inner.list_ci_trusts(workspace_id)
    }

    fn revoke_ci_trust(
        &self,
        id: &str,
        workspace_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        self.check()?;
        self.inner.revoke_ci_trust(id, workspace_id, now_ms, event)
    }

    fn mint_machine_session(
        &self,
        session: &NewMachineSession,
        event: &NewAuditEvent,
    ) -> Result<MachineSessionMintResult, RepositoryError> {
        self.check()?;
        self.inner.mint_machine_session(session, event)
    }

    fn authenticate_machine_session(
        &self,
        token_id: &str,
        now_ms: u64,
    ) -> Result<LocalMachineSessionRecord, RepositoryError> {
        self.check()?;
        self.inner.authenticate_machine_session(token_id, now_ms)
    }
}
