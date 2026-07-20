use crate::{RuntimeStorage, api::AppState, error::ApiError};
use blobyard_contract::{
    AuditValue, LifecycleRepository, NewAuditEvent, ObjectStorage, StorageError, StorageKey,
    WebYardRepository, YardCleanupPlan,
};
use blobyard_repository_sqlite::SqliteRepository;

pub(crate) fn execute_for_yard(
    state: &AppState,
    yard_id: &str,
    completed_at_ms: u64,
) -> Result<(), ApiError> {
    let cleanups = state
        .repository
        .pending_yard_cleanups(Some(yard_id))
        .map_err(ApiError::from_repository)?;
    for cleanup in cleanups {
        let event = completion_event(&cleanup, completed_at_ms);
        crate::lifecycle::execute_deletion(state, &cleanup.deletion, completed_at_ms, &event)?;
    }
    Ok(())
}

pub(crate) fn resume(
    repository: &SqliteRepository,
    storage: &dyn RuntimeStorage,
    completed_at_ms: u64,
) -> Result<(), crate::ServerError> {
    for cleanup in repository.pending_yard_cleanups(None)? {
        delete_bytes(storage, &cleanup)?;
        repository.finish_deletion(
            &cleanup.deletion.id,
            completed_at_ms,
            &completion_event(&cleanup, completed_at_ms),
        )?;
    }
    Ok(())
}

fn delete_bytes(
    storage: &dyn ObjectStorage,
    cleanup: &YardCleanupPlan,
) -> Result<(), crate::ServerError> {
    for item in &cleanup.deletion.items {
        let key = StorageKey::new(item.storage_key.clone())
            .map_err(|_error| crate::ServerError::Initialization)?;
        match storage.delete(&key) {
            Ok(()) | Err(StorageError::NotFound) => {}
            Err(_error) => return Err(crate::ServerError::Storage),
        }
    }
    Ok(())
}

fn completion_event(cleanup: &YardCleanupPlan, completed_at_ms: u64) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_{}", uuid::Uuid::new_v4().simple()),
        workspace_id: cleanup.workspace_id.clone(),
        actor: cleanup.deletion.actor.clone(),
        action: "yard.cleanup_completed".to_owned(),
        request_id: cleanup.deletion.request_id.clone(),
        target_type: "yard_deploy".to_owned(),
        metadata: vec![(
            "deployId".to_owned(),
            AuditValue::String(cleanup.deploy_id.clone()),
        )],
        created_at_ms: completed_at_ms,
    }
}

#[cfg(all(test, feature = "test-seams"))]
#[path = "yard_cleanup_tests.rs"]
mod tests;
