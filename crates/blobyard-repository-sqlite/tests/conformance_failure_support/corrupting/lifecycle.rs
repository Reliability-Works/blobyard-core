use super::{Corrupting, Corruption};
use blobyard_contract::{
    AuditPage, DeletionPlan, LifecycleRepository, NewAuditEvent, NewObjectDeletion,
    RepositoryError, RetentionOverview, RetentionPolicyRecord,
};

impl<T: LifecycleRepository> LifecycleRepository for Corrupting<'_, T> {
    fn record_audit(&self, value: &NewAuditEvent) -> Result<(), RepositoryError> {
        self.inner.record_audit(value)
    }

    fn list_audit(
        &self,
        workspace_id: &str,
        before: Option<u64>,
        limit: u32,
    ) -> Result<AuditPage, RepositoryError> {
        self.inner
            .list_audit(workspace_id, before, limit)
            .map(|mut page| {
                match (self.corruption, before) {
                    (Corruption::AuditPageLength, None)
                    | (Corruption::AuditNextLength, Some(_)) => page.items.clear(),
                    (Corruption::AuditCursor, None) => page.next_before = None,
                    (Corruption::AuditNextAction, Some(_)) => {
                        "changed".clone_into(&mut page.items[0].action);
                    }
                    _ => {}
                }
                page
            })
    }

    fn begin_object_deletion(
        &self,
        value: &NewObjectDeletion,
    ) -> Result<DeletionPlan, RepositoryError> {
        self.inner.begin_object_deletion(value).map(|mut plan| {
            match self.corruption {
                Corruption::DeletionComplete if !plan.complete => plan.complete = true,
                Corruption::DeletionItems if !plan.complete => plan.items.clear(),
                Corruption::DeletionReplayIncomplete if plan.complete => plan.complete = false,
                _ => {}
            }
            plan
        })
    }

    fn finish_deletion(
        &self,
        id: &str,
        completed_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.inner.finish_deletion(id, completed_at_ms, event)
    }

    fn retention_policy(&self, project_id: &str) -> Result<RetentionPolicyRecord, RepositoryError> {
        self.inner.retention_policy(project_id).map(|mut policy| {
            if matches!(self.corruption, Corruption::RetentionPolicy) {
                policy.keep_latest += 1;
            }
            policy
        })
    }

    fn set_retention(
        &self,
        policy: &RetentionPolicyRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.inner.set_retention(policy, event)
    }

    fn clear_retention(
        &self,
        project_id: &str,
        updated_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError> {
        self.inner
            .clear_retention(project_id, updated_at_ms, event)
            .map(|value| {
                if matches!(self.corruption, Corruption::ClearFalse) {
                    false
                } else {
                    value
                }
            })
    }

    fn retention_overview(&self, project_id: &str) -> Result<RetentionOverview, RepositoryError> {
        self.inner
            .retention_overview(project_id)
            .map(|mut overview| {
                if matches!(self.corruption, Corruption::RetentionStatus) {
                    "failed".clone_into(&mut overview.last_run.as_mut().expect("run").status);
                }
                overview
            })
    }

    fn begin_retention(
        &self,
        project_id: &str,
        run_id: &str,
        actor: &str,
        request_id: &str,
        started_at_ms: u64,
    ) -> Result<DeletionPlan, RepositoryError> {
        self.inner
            .begin_retention(project_id, run_id, actor, request_id, started_at_ms)
    }

    fn fail_retention(&self, run_id: &str, completed_at_ms: u64) -> Result<(), RepositoryError> {
        self.inner.fail_retention(run_id, completed_at_ms)
    }

    fn retained_projects(&self) -> Result<Vec<String>, RepositoryError> {
        self.inner.retained_projects()
    }
}
