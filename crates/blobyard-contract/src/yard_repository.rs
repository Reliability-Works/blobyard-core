use crate::{
    DeletionPlan, NewAuditEvent, NewWebYard, NewYardDeploy, NewYardFile, RepositoryError,
    WebYardRecord, YardDeployRecord, YardDeploymentRecord, YardFileTarget, YardStartRecord,
};

/// One durable byte-cleanup plan created when a Web Yard deploy is pruned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardCleanupPlan {
    /// Yard whose retained history caused the cleanup.
    pub yard_id: String,
    /// Workspace used for the redaction-safe completion audit.
    pub workspace_id: String,
    /// Pruned deploy whose immutable object versions are being removed.
    pub deploy_id: String,
    /// Durable storage and metadata deletion plan.
    pub deletion: DeletionPlan,
}

/// Durable standalone operations for named public static sites.
pub trait WebYardRepository: Send + Sync {
    /// Creates or idempotently reuses one Yard and deploy reservation.
    ///
    /// # Errors
    ///
    /// Returns validation, conflict, or provider failures.
    fn start_yard_deploy(
        &self,
        yard: &NewWebYard,
        deploy: &NewYardDeploy,
        created_event: &NewAuditEvent,
    ) -> Result<YardStartRecord, RepositoryError>;

    /// Lists every non-deleted Yard for one project in newest-first order.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures.
    fn list_web_yards(&self, project_id: &str) -> Result<Vec<WebYardRecord>, RepositoryError>;

    /// Reads one Yard, including a deleted record needed for idempotent cleanup.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or provider failures.
    fn web_yard_by_id(&self, yard_id: &str) -> Result<WebYardRecord, RepositoryError>;

    /// Lists deploy history in newest-first order.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures.
    fn list_yard_deploys(&self, yard_id: &str) -> Result<Vec<YardDeployRecord>, RepositoryError>;

    /// Reads one deploy by stable identifier.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or provider failures.
    fn yard_deploy_by_id(&self, deploy_id: &str) -> Result<YardDeployRecord, RepositoryError>;

    /// Atomically snapshots, finalises, promotes, audits, and prunes one deploy.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, conflict, integrity, or provider failures.
    fn finalise_yard_deploy(
        &self,
        deploy_id: &str,
        files: &[NewYardFile],
        finalised_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardDeploymentRecord, RepositoryError>;

    /// Marks an incomplete deploy failed. Repeated calls are idempotent.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, conflict, or provider failures.
    fn fail_yard_deploy(
        &self,
        deploy_id: &str,
        failure_code: &str,
        failure_message: &str,
        failed_at_ms: u64,
    ) -> Result<YardDeployRecord, RepositoryError>;

    /// Atomically selects an earlier retained deploy and records its audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, conflict, or provider failures.
    fn rollback_web_yard(
        &self,
        yard_id: &str,
        deploy_id: Option<&str>,
        rolled_back_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardDeploymentRecord, RepositoryError>;

    /// Atomically deletes one Yard, prunes every deploy, and records its audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, conflict, or provider failures.
    fn delete_web_yard(
        &self,
        yard_id: &str,
        deleted_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError>;

    /// Lists pending byte-cleanup plans, optionally restricted to one Yard.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures.
    fn pending_yard_cleanups(
        &self,
        yard_id: Option<&str>,
    ) -> Result<Vec<YardCleanupPlan>, RepositoryError>;

    /// Resolves an active stable or retained immutable host and normalized request path.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, integrity, or provider failures.
    fn yard_file_by_host(
        &self,
        host_label: &str,
        normalized_request_path: &str,
    ) -> Result<YardFileTarget, RepositoryError>;
}
