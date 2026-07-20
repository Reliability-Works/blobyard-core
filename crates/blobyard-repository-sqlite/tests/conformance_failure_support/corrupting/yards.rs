use super::{Corrupting, Corruption};
use blobyard_contract::{
    NewAuditEvent, NewWebYard, NewYardDeploy, NewYardFile, RepositoryError, WebYardRecord,
    WebYardRepository, WebYardStatus, YardCleanupPlan, YardDeployRecord, YardDeployStatus,
    YardDeploymentRecord, YardFileTarget, YardStartRecord,
};
use blobyard_core::Slug;

impl<T: WebYardRepository> WebYardRepository for Corrupting<'_, T> {
    fn start_yard_deploy(
        &self,
        yard: &NewWebYard,
        deploy: &NewYardDeploy,
        event: &NewAuditEvent,
    ) -> Result<YardStartRecord, RepositoryError> {
        self.inner
            .start_yard_deploy(yard, deploy, event)
            .map(|mut record| {
                if matches!(self.corruption, Corruption::YardReusedStart)
                    && event.created_at_ms == 99
                {
                    record.deploy.id.push_str("_corrupt");
                }
                record
            })
    }

    fn list_web_yards(&self, project_id: &str) -> Result<Vec<WebYardRecord>, RepositoryError> {
        let mut records = self.inner.list_web_yards(project_id)?;
        match self.corruption {
            Corruption::YardInitialList if records.is_empty() => {
                records.push(unexpected_yard(project_id)?);
            }
            Corruption::YardListShape if !records.is_empty() => records.clear(),
            _ => {}
        }
        Ok(records)
    }

    fn web_yard_by_id(&self, yard_id: &str) -> Result<WebYardRecord, RepositoryError> {
        self.inner.web_yard_by_id(yard_id).map(|mut record| {
            if matches!(self.corruption, Corruption::YardFinalRecord)
                && record.status == WebYardStatus::Deleted
            {
                record.status = WebYardStatus::Active;
            }
            record
        })
    }

    fn list_yard_deploys(&self, yard_id: &str) -> Result<Vec<YardDeployRecord>, RepositoryError> {
        self.inner.list_yard_deploys(yard_id)
    }

    fn yard_deploy_by_id(&self, deploy_id: &str) -> Result<YardDeployRecord, RepositoryError> {
        self.inner.yard_deploy_by_id(deploy_id)
    }

    fn finalise_yard_deploy(
        &self,
        deploy_id: &str,
        files: &[NewYardFile],
        finalised_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardDeploymentRecord, RepositoryError> {
        self.inner
            .finalise_yard_deploy(deploy_id, files, finalised_at_ms, event)
            .map(|mut record| {
                if matches!(self.corruption, Corruption::YardReplacementStatus)
                    && finalised_at_ms == 20
                {
                    record.deploy.status = YardDeployStatus::Uploading;
                } else if matches!(self.corruption, Corruption::YardDelayedStatus)
                    && finalised_at_ms == 26
                {
                    record.deploy.status = YardDeployStatus::Live;
                }
                record
            })
    }

    fn fail_yard_deploy(
        &self,
        deploy_id: &str,
        failure_code: &str,
        failure_message: &str,
        failed_at_ms: u64,
    ) -> Result<YardDeployRecord, RepositoryError> {
        self.inner
            .fail_yard_deploy(deploy_id, failure_code, failure_message, failed_at_ms)
            .map(|mut record| {
                if matches!(self.corruption, Corruption::YardFailureRecord) && failed_at_ms == 40 {
                    record.status = YardDeployStatus::Uploading;
                }
                record
            })
    }

    fn rollback_web_yard(
        &self,
        yard_id: &str,
        deploy_id: Option<&str>,
        rolled_back_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardDeploymentRecord, RepositoryError> {
        self.inner
            .rollback_web_yard(yard_id, deploy_id, rolled_back_at_ms, event)
            .map(|mut record| {
                if matches!(self.corruption, Corruption::YardRollbackRecord) {
                    record.yard.current_deploy_id = None;
                }
                record
            })
    }

    fn delete_web_yard(
        &self,
        yard_id: &str,
        deleted_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        self.inner
            .delete_web_yard(yard_id, deleted_at_ms, event)
            .map(|deleted| match self.corruption {
                Corruption::YardFirstDelete if deleted_at_ms == 100 => false,
                Corruption::YardSecondDelete if deleted_at_ms == 101 => true,
                _ => deleted,
            })
    }

    fn pending_yard_cleanups(
        &self,
        yard_id: Option<&str>,
    ) -> Result<Vec<YardCleanupPlan>, RepositoryError> {
        self.inner.pending_yard_cleanups(yard_id)
    }

    fn yard_file_by_host(
        &self,
        host_label: &str,
        normalized_request_path: &str,
    ) -> Result<YardFileTarget, RepositoryError> {
        let result = self
            .inner
            .yard_file_by_host(host_label, normalized_request_path);
        match self.corruption {
            Corruption::YardDeliveryTarget if normalized_request_path.is_empty() => {
                result.map(|mut target| {
                    target.not_found_document = true;
                    target
                })
            }
            Corruption::YardDeletedResolution
                if host_label == "docs-123456789-fixture-1"
                    && result == Err(RepositoryError::NotFound) =>
            {
                Err(RepositoryError::Unavailable)
            }
            _ => result,
        }
    }
}

fn unexpected_yard(project_id: &str) -> Result<WebYardRecord, RepositoryError> {
    Ok(WebYardRecord {
        id: "unexpected".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: project_id.to_owned(),
        name: Slug::new("unexpected").map_err(|_error| RepositoryError::InvalidInput)?,
        host_label: "unexpected-host".to_owned(),
        current_deploy_id: None,
        status: WebYardStatus::Active,
        created_at_ms: 0,
        updated_at_ms: 0,
        deleted_at_ms: None,
    })
}
