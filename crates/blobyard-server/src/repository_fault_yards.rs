use super::FaultingRepository;
use blobyard_contract::{
    NewAuditEvent, NewWebYard, NewYardDeploy, NewYardFile, RepositoryError, WebYardRecord,
    WebYardRepository, YardCleanupPlan, YardDeployRecord, YardDeploymentRecord, YardFileTarget,
    YardStartRecord,
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
    ) -> Result<YardFileTarget, RepositoryError> {
        self.check()?;
        self.inner
            .yard_file_by_host(host_label, normalized_request_path)
    }
}
