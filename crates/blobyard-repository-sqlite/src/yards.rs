use super::{
    SqliteRepository, map_error, rows, yard_cleanup, yard_finalise, yard_lifecycle, yard_queries,
    yard_start,
};
use blobyard_contract::{
    NewAuditEvent, NewWebYard, NewYardDeploy, NewYardFile, RepositoryError, WebYardRecord,
    WebYardRepository, YardCleanupPlan, YardDeployRecord, YardDeploymentRecord, YardFileTarget,
    YardStartRecord, is_valid_yard_request_path,
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

    fn yard_deploy_by_id(&self, deploy_id: &str) -> Result<YardDeployRecord, RepositoryError> {
        rows::validate_text(deploy_id)?;
        let connection = self.connection()?;
        yard_queries::deploy_by_id(&connection, deploy_id)
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
    ) -> Result<YardFileTarget, RepositoryError> {
        rows::validate_text(host_label)?;
        if !is_valid_yard_request_path(normalized_request_path) {
            return Err(RepositoryError::InvalidInput);
        }
        let connection = self.connection()?;
        yard_queries::public_file(&connection, host_label, normalized_request_path)
    }
}

#[cfg(test)]
#[path = "yards_tests.rs"]
mod tests;
