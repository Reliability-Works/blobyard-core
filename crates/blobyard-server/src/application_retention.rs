use crate::{RuntimeStorage, ServerError, storage_configuration::StorageConfiguration};
use blobyard_contract::{
    AuditValue, LifecycleRepository, MetadataRepository, NewAuditEvent, ObjectStorage,
    RepositoryError, StorageError, StorageKey,
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
    for project_id in repository.retained_projects()? {
        enforce_project(&repository, storage.as_ref(), &project_id)?;
    }
    Ok(())
}

fn enforce_project(
    repository: &SqliteRepository,
    storage: &dyn RuntimeStorage,
    project_id: &str,
) -> Result<(), ServerError> {
    enforce_project_with_clock(repository, storage, project_id, current_time)
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
