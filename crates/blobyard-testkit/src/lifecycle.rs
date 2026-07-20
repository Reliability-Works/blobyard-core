use blobyard_contract::{
    AuditValue, LifecycleRepository, NewAuditEvent, NewObjectDeletion, ObjectDeletionTarget,
    RepositoryError, RetentionPolicyRecord,
};

/// Runs deterministic object lifecycle, retention, and audit transitions.
///
/// # Errors
///
/// Returns the first contract failure reported by the adapter.
pub fn lifecycle_conformance(repository: &dyn LifecycleRepository) -> Result<(), RepositoryError> {
    let deletion = deletion();
    let deletion_plan = repository.begin_object_deletion(&deletion)?;
    if deletion_plan.complete || deletion_plan.items.len() != 1 {
        return Err(RepositoryError::Unavailable);
    }
    repository.finish_deletion(
        &deletion.id,
        1_001,
        &event("audit_delete", "object.deleted", "request_delete", 1_001),
    )?;
    if !repository.begin_object_deletion(&deletion)?.complete {
        return Err(RepositoryError::Unavailable);
    }
    let policy = policy();
    repository.set_retention(
        &policy,
        &event(
            "audit_policy",
            "retention.policy_set",
            "request_policy",
            2_000,
        ),
    )?;
    if repository.retention_policy(&policy.project_id)? != policy {
        return Err(RepositoryError::Unavailable);
    }
    enforce(repository, &policy)?;
    assert_audit(repository)?;
    if !repository.clear_retention(
        &policy.project_id,
        2_003,
        &event(
            "audit_clear",
            "retention.policy_cleared",
            "request_clear",
            2_003,
        ),
    )? {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn deletion() -> NewObjectDeletion {
    NewObjectDeletion {
        id: "delete_fixture".to_owned(),
        target: ObjectDeletionTarget {
            project_id: "project_fixture".to_owned(),
            object_path: "reports/example.txt".to_owned(),
            version: Some(1),
        },
        actor: "token_fixture".to_owned(),
        request_id: "request_delete".to_owned(),
        created_at_ms: 1_000,
    }
}

fn policy() -> RetentionPolicyRecord {
    RetentionPolicyRecord {
        project_id: "project_fixture".to_owned(),
        keep_latest: 1,
        path_glob: Some("artifacts/**".to_owned()),
        branch_glob: None,
        created_at_ms: 2_000,
        updated_at_ms: 2_000,
    }
}

fn enforce(
    repository: &dyn LifecycleRepository,
    policy: &RetentionPolicyRecord,
) -> Result<(), RepositoryError> {
    let plan = repository.begin_retention(
        &policy.project_id,
        "retention_fixture",
        "system:retention",
        "request_retention",
        2_001,
    )?;
    repository.finish_deletion(
        &plan.id,
        2_002,
        &event(
            "audit_retention",
            "retention.enforced",
            "request_retention",
            2_002,
        ),
    )?;
    let overview = repository.retention_overview(&policy.project_id)?;
    if overview.last_run.as_ref().map(|run| run.status.as_str()) == Some("complete") {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

fn assert_audit(repository: &dyn LifecycleRepository) -> Result<(), RepositoryError> {
    let page = repository.list_audit("workspace_fixture", None, 2)?;
    if page.items.len() != 2
        || page.next_before.is_none()
        || page.items[0].action != "retention.enforced"
        || page.items[1].action != "retention.policy_set"
    {
        return Err(RepositoryError::Unavailable);
    }
    let next = repository.list_audit("workspace_fixture", page.next_before, 2)?;
    if next
        .items
        .first()
        .is_some_and(|event| event.action == "object.deleted")
    {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

fn event(id: &str, action: &str, request_id: &str, created_at_ms: u64) -> NewAuditEvent {
    NewAuditEvent {
        id: id.to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: if action == "retention.enforced" {
            "system:retention".to_owned()
        } else {
            "token_fixture".to_owned()
        },
        action: action.to_owned(),
        request_id: request_id.to_owned(),
        target_type: "fixture".to_owned(),
        metadata: vec![("fixture".to_owned(), AuditValue::Boolean(true))],
        created_at_ms,
    }
}
