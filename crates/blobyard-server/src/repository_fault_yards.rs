use super::{Corruption, FaultingRepository};
use blobyard_contract::{
    NewAuditEvent, NewWebYard, NewYardAccessGrant, NewYardDeploy, NewYardFile, RepositoryError,
    WebYardRecord, WebYardRepository, YardAccessGrantRecord, YardAccessPolicyRecord,
    YardCleanupPlan, YardDeployRecord, YardDeploymentRecord, YardEnvironmentRecord, YardFileTarget,
    YardStartRecord, YardVisibility,
};

impl WebYardRepository for FaultingRepository {
    fn start_yard_deploy(
        &self,
        yard: &NewWebYard,
        deploy: &NewYardDeploy,
        event: &NewAuditEvent,
    ) -> Result<YardStartRecord, RepositoryError> {
        self.check()?;
        self.inner.start_yard_deploy(yard, deploy, event)
    }

    fn list_web_yards(&self, project_id: &str) -> Result<Vec<WebYardRecord>, RepositoryError> {
        self.check()?;
        self.inner.list_web_yards(project_id)
    }

    fn web_yard_by_id(&self, yard_id: &str) -> Result<WebYardRecord, RepositoryError> {
        self.check()?;
        self.inner.web_yard_by_id(yard_id)
    }

    fn list_yard_deploys(&self, yard_id: &str) -> Result<Vec<YardDeployRecord>, RepositoryError> {
        self.check()?;
        self.inner.list_yard_deploys(yard_id)
    }

    fn yard_deploy_by_id(&self, deploy_id: &str) -> Result<YardDeployRecord, RepositoryError> {
        self.check()?;
        self.inner.yard_deploy_by_id(deploy_id)
    }

    fn list_yard_environments(
        &self,
        yard_id: &str,
    ) -> Result<Vec<YardEnvironmentRecord>, RepositoryError> {
        self.check()?;
        self.inner.list_yard_environments(yard_id)
    }

    fn get_yard_access_policy(
        &self,
        yard_id: &str,
    ) -> Result<Option<YardAccessPolicyRecord>, RepositoryError> {
        self.check()?;
        self.inner.get_yard_access_policy(yard_id)
    }

    fn set_yard_visibility(
        &self,
        yard_id: &str,
        visibility: YardVisibility,
        updated_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardAccessPolicyRecord, RepositoryError> {
        self.check()?;
        self.inner
            .set_yard_visibility(yard_id, visibility, updated_at_ms, event)
    }

    fn insert_yard_access_grant(
        &self,
        grant: &NewYardAccessGrant,
        event: &NewAuditEvent,
    ) -> Result<YardAccessGrantRecord, RepositoryError> {
        self.check()?;
        self.inner.insert_yard_access_grant(grant, event)
    }

    fn revoke_yard_access_grant(
        &self,
        yard_id: &str,
        grant_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        self.check()?;
        self.inner
            .revoke_yard_access_grant(yard_id, grant_id, revoked_at_ms, event)
    }

    fn list_yard_access_grants(
        &self,
        yard_id: &str,
        now_ms: u64,
    ) -> Result<Vec<YardAccessGrantRecord>, RepositoryError> {
        self.check()?;
        let mut grants = self.inner.list_yard_access_grants(yard_id, now_ms)?;
        if matches!(self.corruption, Some(Corruption::YardAccessGrantTimestamp))
            && let Some(grant) = grants.first_mut()
        {
            grant.created_at_ms = u64::MAX;
        }
        Ok(grants)
    }

    fn finalise_yard_deploy(
        &self,
        deploy_id: &str,
        files: &[NewYardFile],
        finalised_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardDeploymentRecord, RepositoryError> {
        self.check()?;
        self.inner
            .finalise_yard_deploy(deploy_id, files, finalised_at_ms, event)
    }

    fn fail_yard_deploy(
        &self,
        deploy_id: &str,
        failure_code: &str,
        failure_message: &str,
        failed_at_ms: u64,
    ) -> Result<YardDeployRecord, RepositoryError> {
        self.check()?;
        self.inner
            .fail_yard_deploy(deploy_id, failure_code, failure_message, failed_at_ms)
    }

    fn rollback_web_yard(
        &self,
        yard_id: &str,
        deploy_id: Option<&str>,
        rolled_back_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardDeploymentRecord, RepositoryError> {
        self.check()?;
        self.inner
            .rollback_web_yard(yard_id, deploy_id, rolled_back_at_ms, event)
    }

    fn delete_web_yard(
        &self,
        yard_id: &str,
        deleted_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        self.check()?;
        self.inner.delete_web_yard(yard_id, deleted_at_ms, event)
    }

    fn pending_yard_cleanups(
        &self,
        yard_id: Option<&str>,
    ) -> Result<Vec<YardCleanupPlan>, RepositoryError> {
        self.check()?;
        self.inner.pending_yard_cleanups(yard_id)
    }

    fn yard_file_by_host(
        &self,
        host_label: &str,
        normalized_request_path: &str,
        session_token_hash: Option<&str>,
        now_ms: u64,
    ) -> Result<YardFileTarget, RepositoryError> {
        self.check()?;
        self.inner.yard_file_by_host(
            host_label,
            normalized_request_path,
            session_token_hash,
            now_ms,
        )
    }
}
