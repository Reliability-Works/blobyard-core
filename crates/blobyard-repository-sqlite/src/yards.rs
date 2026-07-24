use super::{
    SqliteRepository, map_error, rows, transfer_validation, yard_access, yard_cleanup,
    yard_finalise, yard_lifecycle, yard_queries, yard_start,
};
use blobyard_contract::{
    NewAuditEvent, NewWebYard, NewYardAccessGrant, NewYardDeploy, NewYardFile, RepositoryError,
    WebYardRecord, WebYardRepository, YardAccessGrantRecord, YardAccessPolicyRecord,
    YardCleanupPlan, YardDeployRecord, YardDeploymentRecord, YardEnvironmentRecord, YardFileTarget,
    YardStartRecord, YardVisibility, is_valid_yard_request_path,
};

impl WebYardRepository for SqliteRepository {
    fn start_yard_deploy(
        &self,
        yard: &NewWebYard,
        deploy: &NewYardDeploy,
        created_event: &NewAuditEvent,
    ) -> Result<YardStartRecord, RepositoryError> {
        self.write_transaction(|transaction| {
            yard_start::start(transaction, yard, deploy, created_event)
        })
    }

    fn list_web_yards(&self, project_id: &str) -> Result<Vec<WebYardRecord>, RepositoryError> {
        rows::validate_text(project_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {} FROM web_yards WHERE project_id = ?1 AND status != 'deleted' ORDER BY created_at_ms DESC, id DESC",
                super::yard_rows::YARD_COLUMNS
            ))
            .map_err(map_error)?;
        let result = yard_queries::list_yards(&mut statement, project_id);
        drop(statement);
        drop(connection);
        result
    }

    fn web_yard_by_id(&self, yard_id: &str) -> Result<WebYardRecord, RepositoryError> {
        rows::validate_text(yard_id)?;
        let connection = self.connection()?;
        yard_queries::yard_by_id(&connection, yard_id)
    }

    fn list_yard_deploys(&self, yard_id: &str) -> Result<Vec<YardDeployRecord>, RepositoryError> {
        rows::validate_text(yard_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {} FROM yard_deploys WHERE yard_id = ?1 ORDER BY created_at_ms DESC, id DESC",
                super::yard_rows::DEPLOY_COLUMNS
            ))
            .map_err(map_error)?;
        let result = yard_queries::list_deploys(&mut statement, yard_id);
        drop(statement);
        drop(connection);
        result
    }

    fn list_yard_environments(
        &self,
        yard_id: &str,
    ) -> Result<Vec<YardEnvironmentRecord>, RepositoryError> {
        rows::validate_text(yard_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {} FROM yard_environments WHERE yard_id = ?1 AND status != 'deleted' ORDER BY kind != 'production', name",
                super::yard_rows::ENVIRONMENT_COLUMNS
            ))
            .map_err(map_error)?;
        let result = yard_queries::list_environments(&mut statement, yard_id);
        drop(statement);
        drop(connection);
        result
    }

    fn yard_deploy_by_id(&self, deploy_id: &str) -> Result<YardDeployRecord, RepositoryError> {
        rows::validate_text(deploy_id)?;
        let connection = self.connection()?;
        yard_queries::deploy_by_id(&connection, deploy_id)
    }

    fn get_yard_access_policy(
        &self,
        yard_id: &str,
    ) -> Result<Option<YardAccessPolicyRecord>, RepositoryError> {
        rows::validate_text(yard_id)?;
        let connection = self.connection()?;
        yard_access::policy(&connection, yard_id)
    }

    fn set_yard_visibility(
        &self,
        yard_id: &str,
        visibility: YardVisibility,
        updated_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardAccessPolicyRecord, RepositoryError> {
        rows::validate_text(yard_id)?;
        self.write_transaction(|transaction| {
            yard_access::set_visibility(transaction, yard_id, visibility, updated_at_ms, event)
        })
    }

    fn insert_yard_access_grant(
        &self,
        grant: &NewYardAccessGrant,
        event: &NewAuditEvent,
    ) -> Result<YardAccessGrantRecord, RepositoryError> {
        self.write_transaction(|transaction| yard_access::insert_grant(transaction, grant, event))
    }

    fn revoke_yard_access_grant(
        &self,
        yard_id: &str,
        grant_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        rows::validate_text(yard_id)?;
        rows::validate_text(grant_id)?;
        self.write_transaction(|transaction| {
            yard_access::revoke_grant(transaction, yard_id, grant_id, revoked_at_ms, event)
        })
    }

    fn list_yard_access_grants(
        &self,
        yard_id: &str,
        now_ms: u64,
    ) -> Result<Vec<YardAccessGrantRecord>, RepositoryError> {
        rows::validate_text(yard_id)?;
        let now = transfer_validation::to_i64(now_ms)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {} FROM yard_access_grants WHERE yard_id = ?1 AND status = 'active' AND (expires_at_ms IS NULL OR expires_at_ms > ?2) ORDER BY created_at_ms DESC, id DESC",
                yard_access::GRANT_COLUMNS
            ))
            .map_err(map_error)?;
        let result = yard_access::list_grants(&mut statement, yard_id, now);
        drop(statement);
        drop(connection);
        result
    }

    fn finalise_yard_deploy(
        &self,
        deploy_id: &str,
        files: &[NewYardFile],
        finalised_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardDeploymentRecord, RepositoryError> {
        rows::validate_text(deploy_id)?;
        self.write_transaction(|transaction| {
            yard_finalise::finalise(transaction, deploy_id, files, finalised_at_ms, event)
        })
    }

    fn fail_yard_deploy(
        &self,
        deploy_id: &str,
        failure_code: &str,
        failure_message: &str,
        failed_at_ms: u64,
    ) -> Result<YardDeployRecord, RepositoryError> {
        rows::validate_text(deploy_id)?;
        self.write_transaction(|transaction| {
            yard_lifecycle::fail(
                transaction,
                deploy_id,
                failure_code,
                failure_message,
                failed_at_ms,
            )
        })
    }

    fn rollback_web_yard(
        &self,
        yard_id: &str,
        deploy_id: Option<&str>,
        rolled_back_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardDeploymentRecord, RepositoryError> {
        rows::validate_text(yard_id)?;
        if let Some(deploy_id) = deploy_id {
            rows::validate_text(deploy_id)?;
        }
        self.write_transaction(|transaction| {
            yard_lifecycle::rollback(transaction, yard_id, deploy_id, rolled_back_at_ms, event)
        })
    }

    fn delete_web_yard(
        &self,
        yard_id: &str,
        deleted_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        rows::validate_text(yard_id)?;
        self.write_transaction(|transaction| {
            yard_lifecycle::delete(transaction, yard_id, deleted_at_ms, event)
        })
    }

    fn pending_yard_cleanups(
        &self,
        yard_id: Option<&str>,
    ) -> Result<Vec<YardCleanupPlan>, RepositoryError> {
        if let Some(yard_id) = yard_id {
            rows::validate_text(yard_id)?;
        }
        self.connection()
            .and_then(|connection| yard_cleanup::pending(&connection, yard_id))
    }

    fn yard_file_by_host(
        &self,
        host_label: &str,
        normalized_request_path: &str,
        session_token_hash: Option<&str>,
        now_ms: u64,
    ) -> Result<YardFileTarget, RepositoryError> {
        rows::validate_text(host_label)?;
        if !is_valid_yard_request_path(normalized_request_path) {
            return Err(RepositoryError::InvalidInput);
        }
        let now = transfer_validation::to_i64(now_ms)?;
        self.write_transaction(|transaction| {
            yard_queries::authorized_file(
                transaction,
                host_label,
                normalized_request_path,
                session_token_hash,
                now,
            )
        })
    }
}

#[cfg(test)]
#[path = "yards_tests.rs"]
mod tests;
