use super::{
    SqliteRepository, lifecycle_audit, lifecycle_deletion, lifecycle_retention,
    lifecycle_retention_plan, rows,
};
use blobyard_contract::{
    AuditPage, DeletionPlan, LifecycleRepository, NewAuditEvent, NewObjectDeletion,
    RepositoryError, RetentionOverview, RetentionPolicyRecord,
};

impl LifecycleRepository for SqliteRepository {
    fn record_audit(&self, event: &NewAuditEvent) -> Result<(), RepositoryError> {
        self.connection()
            .and_then(|connection| lifecycle_audit::insert(&connection, event))
    }

    fn list_audit(
        &self,
        workspace_id: &str,
        before: Option<u64>,
        limit: u32,
    ) -> Result<AuditPage, RepositoryError> {
        self.connection()
            .and_then(|connection| lifecycle_audit::list(&connection, workspace_id, before, limit))
    }

    fn begin_object_deletion(
        &self,
        deletion: &NewObjectDeletion,
    ) -> Result<DeletionPlan, RepositoryError> {
        let validated = lifecycle_deletion::validate(deletion)?;
        self.write_transaction(|transaction| {
            lifecycle_deletion::begin(transaction, deletion, validated)
        })
    }

    fn finish_deletion(
        &self,
        operation_id: &str,
        completed_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        rows::validate_text(operation_id)?;
        let completed_at_ms = lifecycle_audit::to_i64(completed_at_ms)?;
        self.write_transaction(|transaction| {
            lifecycle_deletion::finish(transaction, operation_id, completed_at_ms, event)
        })
    }

    fn retention_policy(&self, project_id: &str) -> Result<RetentionPolicyRecord, RepositoryError> {
        self.connection()
            .and_then(|connection| lifecycle_retention::policy(&connection, project_id))
    }

    fn set_retention(
        &self,
        policy: &RetentionPolicyRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.write_transaction(|transaction| lifecycle_retention::set(transaction, policy, event))
    }

    fn clear_retention(
        &self,
        project_id: &str,
        updated_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        self.write_transaction(|transaction| {
            lifecycle_retention::clear(transaction, project_id, updated_at_ms, event)
        })
    }

    fn retention_overview(&self, project_id: &str) -> Result<RetentionOverview, RepositoryError> {
        self.connection()
            .and_then(|connection| lifecycle_retention::overview(&connection, project_id))
    }

    fn begin_retention(
        &self,
        project_id: &str,
        run_id: &str,
        actor: &str,
        request_id: &str,
        started_at_ms: u64,
    ) -> Result<DeletionPlan, RepositoryError> {
        self.write_transaction(|transaction| {
            lifecycle_retention_plan::begin(
                transaction,
                project_id,
                run_id,
                actor,
                request_id,
                started_at_ms,
            )
        })
    }

    fn fail_retention(&self, run_id: &str, completed_at_ms: u64) -> Result<(), RepositoryError> {
        self.connection()
            .and_then(|connection| lifecycle_retention::fail(&connection, run_id, completed_at_ms))
    }

    fn retained_projects(&self) -> Result<Vec<String>, RepositoryError> {
        self.connection()
            .and_then(|connection| lifecycle_retention::projects(&connection))
    }
}
