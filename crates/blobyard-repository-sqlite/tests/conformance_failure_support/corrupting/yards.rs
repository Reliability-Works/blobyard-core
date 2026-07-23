use super::{Corrupting, Corruption, yard_access};
use blobyard_contract::{
    NewAuditEvent, NewWebYard, NewYardAccessGrant, NewYardDeploy, NewYardFile, RepositoryError,
    WebYardRecord, WebYardRepository, WebYardStatus, YardAccessGrantRecord, YardAccessPolicyRecord,
    YardCleanupPlan, YardDeployRecord, YardDeployStatus, YardDeploymentRecord,
    YardEnvironmentRecord, YardFileTarget, YardStartRecord, YardVisibility,
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

    fn list_yard_environments(
        &self,
        yard_id: &str,
    ) -> Result<Vec<YardEnvironmentRecord>, RepositoryError> {
        let mut records = self.inner.list_yard_environments(yard_id)?;
        match self.corruption {
            Corruption::YardEnvironmentList if !records.is_empty() => records.clear(),
            Corruption::YardUnknownEnvironmentList if records.is_empty() => {
                records.push(unexpected_environment(yard_id)?);
            }
            _ => {}
        }
        Ok(records)
    }

    fn get_yard_access_policy(
        &self,
        yard_id: &str,
    ) -> Result<Option<YardAccessPolicyRecord>, RepositoryError> {
        let record = self.inner.get_yard_access_policy(yard_id)?;
        Ok(yard_access::corrupt_policy(
            self.corruption,
            yard_id,
            record,
        ))
    }

    fn set_yard_visibility(
        &self,
        yard_id: &str,
        visibility: YardVisibility,
        updated_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardAccessPolicyRecord, RepositoryError> {
        self.inner
            .set_yard_visibility(yard_id, visibility, updated_at_ms, event)
            .map(|record| yard_access::corrupt_visibility(self.corruption, updated_at_ms, record))
    }

    fn insert_yard_access_grant(
        &self,
        grant: &NewYardAccessGrant,
        event: &NewAuditEvent,
    ) -> Result<YardAccessGrantRecord, RepositoryError> {
        let result = self.inner.insert_yard_access_grant(grant, event);
        yard_access::corrupt_inserted_grant(self.corruption, grant.created_at_ms, result)
    }

    fn revoke_yard_access_grant(
        &self,
        yard_id: &str,
        grant_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        let result = self
            .inner
            .revoke_yard_access_grant(yard_id, grant_id, revoked_at_ms, event);
        yard_access::corrupt_revocation(self.corruption, grant_id, revoked_at_ms, result)
    }

    fn list_yard_access_grants(
        &self,
        yard_id: &str,
        now_ms: u64,
    ) -> Result<Vec<YardAccessGrantRecord>, RepositoryError> {
        let records = self.inner.list_yard_access_grants(yard_id, now_ms)?;
        Ok(yard_access::corrupt_grant_list(
            self.corruption,
            yard_id,
            now_ms,
            records,
        ))
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
            Corruption::YardPrivateDelivery
                if normalized_request_path == "asset.js"
                    && result == Err(RepositoryError::NotFound) =>
            {
                Err(RepositoryError::Unavailable)
            }
            _ => result,
        }
    }
}

fn unexpected_environment(yard_id: &str) -> Result<YardEnvironmentRecord, RepositoryError> {
    Ok(YardEnvironmentRecord {
        id: "yardenv_unexpected".to_owned(),
        yard_id: yard_id.to_owned(),
        name: Slug::new("production").map_err(|_error| RepositoryError::InvalidInput)?,
        kind: blobyard_contract::YardEnvironmentKind::Production,
        status: blobyard_contract::YardEnvironmentStatus::Active,
        created_at_ms: 0,
        updated_at_ms: 0,
    })
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
