use crate::{api::AppState, error::ApiError};
use blobyard_contract::{DeletionPlan, NewAuditEvent, StorageError, StorageKey};

pub(crate) fn execute_deletion(
    state: &AppState,
    plan: &DeletionPlan,
    completed_at_ms: u64,
    event: &NewAuditEvent,
) -> Result<(), ApiError> {
    if !plan.complete {
        for item in &plan.items {
            let key = match StorageKey::new(item.storage_key.clone()) {
                Ok(key) => key,
                Err(_error) => return Err(ApiError::internal()),
            };
            match state.storage.delete(&key) {
                Ok(()) | Err(StorageError::NotFound) => {}
                Err(error) => return Err(ApiError::from_storage(error)),
            }
        }
        state
            .repository
            .finish_deletion(&plan.id, completed_at_ms, event)
            .map_err(ApiError::from_repository)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::execute_deletion;
    use crate::test_support;
    use axum::{http::StatusCode, response::IntoResponse};
    use blobyard_contract::{DeletionItem, DeletionPlan, NewAuditEvent};

    #[test]
    fn corrupt_storage_key_is_internal_and_does_not_finalize_deletion() {
        let root = tempfile::tempdir().expect("root");
        let state = test_support::filesystem_state(&root, root.path().join("staging"));
        let mut plan = DeletionPlan {
            id: "missing_operation".to_owned(),
            items: vec![DeletionItem {
                version_id: "version_fixture".to_owned(),
                storage_key: "../invalid".to_owned(),
                version: 1,
            }],
            complete: false,
            actor: "fixture".to_owned(),
            request_id: "request_fixture".to_owned(),
        };
        let event = NewAuditEvent {
            id: "event_fixture".to_owned(),
            workspace_id: "workspace_fixture".to_owned(),
            actor: "fixture".to_owned(),
            action: "object.deleted".to_owned(),
            request_id: "request_fixture".to_owned(),
            target_type: "object".to_owned(),
            metadata: Vec::new(),
            created_at_ms: 1,
        };

        let response = execute_deletion(&state, &plan, 1, &event)
            .expect_err("corrupt key")
            .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            state
                .repository
                .list_audit("workspace_fixture", None, 1)
                .expect("audit query")
                .items
                .len(),
            0
        );

        plan.complete = true;
        assert!(execute_deletion(&state, &plan, 1, &event).is_ok());
        plan.complete = false;
        plan.items[0].storage_key = "valid/key".to_owned();
        let response = execute_deletion(&state, &plan, 1, &event)
            .expect_err("missing bytes continue to repository finalization")
            .into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        std::fs::create_dir_all(root.path().join("objects/objects/valid/key"))
            .expect("storage blocker");
        let response = execute_deletion(&state, &plan, 1, &event)
            .expect_err("storage failure")
            .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
