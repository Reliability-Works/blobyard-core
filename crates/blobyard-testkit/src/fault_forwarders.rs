//! Shared forwarding implementations for failure-injection test adapters.

use blobyard_contract::RepositoryError;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Deterministically rejects the operation at one configured call index.
pub struct FailureCounter {
    remaining: AtomicUsize,
}

impl FailureCounter {
    /// Creates a counter that rejects after `successful_calls` successful checks.
    #[must_use]
    pub const fn new(successful_calls: usize) -> Self {
        Self {
            remaining: AtomicUsize::new(successful_calls),
        }
    }

    /// Advances the counter or returns the stable repository failure.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Unavailable`] at the configured failure index.
    pub fn check(&self) -> Result<(), RepositoryError> {
        self.remaining
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                remaining.checked_sub(1)
            })
            .map(|_| ())
            .map_err(|_| RepositoryError::Unavailable)
    }
}

/// Implements metadata-repository forwarding after the adapter's `check` hook.
#[doc(hidden)]
#[macro_export]
macro_rules! impl_faulting_metadata_repository {
    () => {
        fn schema_version(&self) -> Result<u32, blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.schema_version()
        }

        fn create_workspace(
            &self,
            value: &blobyard_contract::WorkspaceRecord,
        ) -> Result<(), blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.create_workspace(value)
        }

        fn list_workspaces(
            &self,
        ) -> Result<Vec<blobyard_contract::WorkspaceRecord>, blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.list_workspaces()
        }

        fn workspace_by_slug(
            &self,
            slug: &blobyard_core::Slug,
        ) -> Result<blobyard_contract::WorkspaceRecord, blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.workspace_by_slug(slug)
        }

        fn rename_workspace(
            &self,
            value: &blobyard_contract::WorkspaceRecord,
            event: &blobyard_contract::NewAuditEvent,
        ) -> Result<(), blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.rename_workspace(value, event)
        }

        fn create_project(
            &self,
            value: &blobyard_contract::ProjectRecord,
        ) -> Result<(), blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.create_project(value)
        }

        fn list_projects(
            &self,
            workspace_id: &str,
        ) -> Result<Vec<blobyard_contract::ProjectRecord>, blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.list_projects(workspace_id)
        }

        fn project_by_slug(
            &self,
            workspace_id: &str,
            slug: &blobyard_core::Slug,
        ) -> Result<blobyard_contract::ProjectRecord, blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.project_by_slug(workspace_id, slug)
        }

        fn reserve_object_version(
            &self,
            value: &blobyard_contract::NewObjectVersion,
        ) -> Result<(), blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.reserve_object_version(value)
        }

        fn complete_object_version(
            &self,
            id: &str,
            size: u64,
            checksum: &str,
        ) -> Result<(), blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.complete_object_version(id, size, checksum)
        }

        fn abort_object_version(&self, id: &str) -> Result<(), blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.abort_object_version(id)
        }

        fn object_version(
            &self,
            id: &str,
        ) -> Result<blobyard_contract::ObjectVersionRecord, blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.object_version(id)
        }
    };
}

/// Implements lifecycle-repository forwarding after the adapter's `check` hook.
#[doc(hidden)]
#[macro_export]
macro_rules! impl_faulting_lifecycle_repository {
    () => {
        fn record_audit(
            &self,
            value: &blobyard_contract::NewAuditEvent,
        ) -> Result<(), blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.record_audit(value)
        }

        fn list_audit(
            &self,
            workspace_id: &str,
            before: Option<u64>,
            limit: u32,
        ) -> Result<blobyard_contract::AuditPage, blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.list_audit(workspace_id, before, limit)
        }

        fn begin_object_deletion(
            &self,
            value: &blobyard_contract::NewObjectDeletion,
        ) -> Result<blobyard_contract::DeletionPlan, blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.begin_object_deletion(value)
        }

        fn finish_deletion(
            &self,
            id: &str,
            completed_at_ms: u64,
            event: &blobyard_contract::NewAuditEvent,
        ) -> Result<(), blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.finish_deletion(id, completed_at_ms, event)
        }

        fn retention_policy(
            &self,
            project_id: &str,
        ) -> Result<blobyard_contract::RetentionPolicyRecord, blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.retention_policy(project_id)
        }

        fn set_retention(
            &self,
            policy: &blobyard_contract::RetentionPolicyRecord,
            event: &blobyard_contract::NewAuditEvent,
        ) -> Result<(), blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.set_retention(policy, event)
        }

        fn clear_retention(
            &self,
            project_id: &str,
            updated_at_ms: u64,
            event: &blobyard_contract::NewAuditEvent,
        ) -> Result<bool, blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.clear_retention(project_id, updated_at_ms, event)
        }

        fn retention_overview(
            &self,
            project_id: &str,
        ) -> Result<blobyard_contract::RetentionOverview, blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.retention_overview(project_id)
        }

        fn begin_retention(
            &self,
            project_id: &str,
            run_id: &str,
            actor: &str,
            request_id: &str,
            started_at_ms: u64,
        ) -> Result<blobyard_contract::DeletionPlan, blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner
                .begin_retention(project_id, run_id, actor, request_id, started_at_ms)
        }

        fn fail_retention(
            &self,
            run_id: &str,
            completed_at_ms: u64,
        ) -> Result<(), blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.fail_retention(run_id, completed_at_ms)
        }

        fn retained_projects(&self) -> Result<Vec<String>, blobyard_contract::RepositoryError> {
            self.check()?;
            self.inner.retained_projects()
        }
    };
}

/// Implements the multipart-start methods that a storage test adapter forwards unchanged.
#[doc(hidden)]
#[macro_export]
macro_rules! impl_forwarding_multipart_start {
    () => {
        fn begin_multipart(
            &self,
            key: &blobyard_contract::StorageKey,
            expected: &blobyard_contract::StorageMetadata,
        ) -> Result<blobyard_contract::MultipartId, blobyard_contract::StorageError> {
            self.inner.begin_multipart(key, expected)
        }

        fn put_part(
            &self,
            upload: &blobyard_contract::MultipartId,
            number: u32,
            source: &mut dyn std::io::Read,
        ) -> Result<blobyard_contract::MultipartPart, blobyard_contract::StorageError> {
            self.inner.put_part(upload, number, source)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::FailureCounter;
    use blobyard_contract::RepositoryError;

    #[test]
    fn failure_counter_allows_configured_calls_then_fails_closed() {
        let counter = FailureCounter::new(1);

        assert_eq!(counter.check(), Ok(()));
        assert_eq!(counter.check(), Err(RepositoryError::Unavailable));
    }
}
