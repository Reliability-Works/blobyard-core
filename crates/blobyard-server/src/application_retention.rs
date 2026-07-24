use crate::{RuntimeStorage, ServerError, storage_configuration::StorageConfiguration};
use blobyard_contract::{
    AuditValue, LifecycleRepository, MetadataRepository, NewAuditEvent, ObjectStorage,
    RepositoryError, StorageError, StorageKey, YardSessionRepository,
};
use blobyard_repository_sqlite::SqliteRepository;
use std::path::Path;

pub(super) fn enforce_retention(data_directory: &Path) -> Result<(), ServerError> {
    enforce_retention_with_storage(data_directory, &StorageConfiguration::Filesystem)
}

pub(super) fn enforce_retention_with_storage(
    data_directory: &Path,
    storage_configuration: &StorageConfiguration,
) -> Result<(), ServerError> {
    let repository = SqliteRepository::open(&data_directory.join("metadata.sqlite3"))?;
    let storage = storage_configuration.open(data_directory)?;
    enforce_repository_with_housekeeping(
        &repository,
        storage.as_ref(),
        current_time,
        SqliteRepository::purge_yard_session_history,
    )
}

fn enforce_repository_with_housekeeping<F>(
    repository: &SqliteRepository,
    storage: &dyn RuntimeStorage,
    clock: fn() -> Result<u64, ServerError>,
    housekeeping: F,
) -> Result<(), ServerError>
where
    F: FnOnce(&SqliteRepository, u64) -> Result<(), RepositoryError>,
{
    housekeeping(repository, clock()?)?;
    for project_id in repository.retained_projects()? {
        enforce_project_with_clock(repository, storage, &project_id, clock)?;
    }
    Ok(())
}

pub(super) fn enforce_project_with_clock(
    repository: &SqliteRepository,
    storage: &dyn ObjectStorage,
    project_id: &str,
    clock: fn() -> Result<u64, ServerError>,
) -> Result<(), ServerError> {
    let workspace_id = project_workspace(repository, project_id)?;
    let started_at_ms = clock()?;
    let run_id = format!("retention_{}", uuid::Uuid::new_v4().simple());
    let request_id = format!("req_{}", uuid::Uuid::new_v4().simple());
    let plan = repository.begin_retention(
        project_id,
        &run_id,
        "system:retention",
        &request_id,
        started_at_ms,
    )?;
    enforce_plan_with_clock(
        repository,
        storage,
        workspace_id,
        started_at_ms,
        plan,
        clock,
    )
}

pub(super) fn enforce_plan_with_clock(
    repository: &SqliteRepository,
    storage: &dyn ObjectStorage,
    workspace_id: String,
    started_at_ms: u64,
    plan: blobyard_contract::DeletionPlan,
    clock: fn() -> Result<u64, ServerError>,
) -> Result<(), ServerError> {
    if plan.complete {
        return Ok(());
    }
    let mut deleted_count = 0_u64;
    for item in &plan.items {
        let key =
            StorageKey::new(item.storage_key.clone()).map_err(|_error| ServerError::Storage)?;
        match storage.delete(&key) {
            Ok(()) | Err(StorageError::NotFound) => {}
            Err(_error) => {
                repository.fail_retention(&plan.id, started_at_ms)?;
                return Err(ServerError::Storage);
            }
        }
        deleted_count = deleted_count.saturating_add(1);
    }
    finish(repository, workspace_id, deleted_count, plan, clock)
}

fn finish(
    repository: &SqliteRepository,
    workspace_id: String,
    deleted_count: u64,
    plan: blobyard_contract::DeletionPlan,
    clock: fn() -> Result<u64, ServerError>,
) -> Result<(), ServerError> {
    let completed_at_ms = clock()?;
    repository.finish_deletion(
        &plan.id,
        completed_at_ms,
        &NewAuditEvent {
            id: format!("audit_{}", uuid::Uuid::new_v4().simple()),
            workspace_id,
            actor: plan.actor,
            action: "retention.enforced".to_owned(),
            request_id: plan.request_id,
            target_type: "retention_policy".to_owned(),
            metadata: vec![("deletedCount".to_owned(), AuditValue::Number(deleted_count))],
            created_at_ms: completed_at_ms,
        },
    )?;
    Ok(())
}

pub(super) fn current_time() -> Result<u64, ServerError> {
    crate::transfer_grants::now_ms().map_err(|_error| ServerError::Initialization)
}

pub(super) fn project_workspace(
    repository: &SqliteRepository,
    project_id: &str,
) -> Result<String, ServerError> {
    for workspace in repository.list_workspaces()? {
        if repository
            .list_projects(&workspace.id)?
            .iter()
            .any(|project| project.id == project_id)
        {
            return Ok(workspace.id);
        }
    }
    Err(RepositoryError::NotFound.into())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::*;
    use blobyard_storage_filesystem::FilesystemStorage;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn housekeeping_runs_once_without_retained_projects_and_propagates_failure() {
        let root = tempfile::tempdir().expect("root");
        let repository =
            SqliteRepository::open(&root.path().join("metadata.sqlite3")).expect("repository");
        let storage = FilesystemStorage::open(&root.path().join("objects")).expect("storage");
        let calls = AtomicUsize::new(0);

        enforce_repository_with_housekeeping(
            &repository,
            &storage,
            fixed_clock,
            |_repository, _now_ms| {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
        )
        .expect("housekeeping");
        assert_eq!(calls.load(Ordering::Relaxed), 1);

        assert_eq!(
            enforce_repository_with_housekeeping(
                &repository,
                &storage,
                fixed_clock,
                |_repository, _now_ms| Err(RepositoryError::Unavailable),
            ),
            Err(ServerError::Repository(RepositoryError::Unavailable))
        );
    }

    #[allow(
        clippy::unnecessary_wraps,
        reason = "production housekeeping accepts a fallible clock"
    )]
    const fn fixed_clock() -> Result<u64, ServerError> {
        Ok(1_000)
    }
}
