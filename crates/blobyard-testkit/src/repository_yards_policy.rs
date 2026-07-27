use super::YardConformanceRepository;
use blobyard_contract::{AuditValue, NewAuditEvent, RepositoryError, YardManagementRole};

pub(super) fn approve_application_policy(
    repository: &dyn YardConformanceRepository,
    yard_id: &str,
) -> Result<(), RepositoryError> {
    approve_owner(repository, yard_id)?;
    let policy = super::yard_application_policy();
    let digest = "a".repeat(64);
    let policy_event = super::yard_policy_event(yard_id, &digest, 102);
    repository
        .set_yard_application_policy(yard_id, &digest, policy, "fixture", 102, &policy_event)
        .map(|_policy| ())
}

fn approve_owner(
    repository: &dyn YardConformanceRepository,
    yard_id: &str,
) -> Result<(), RepositoryError> {
    let owner_event = super::yard_owner_event(yard_id, "user_fixture", 101);
    repository
        .set_yard_management_role(
            yard_id,
            "user_fixture",
            YardManagementRole::Owner,
            101,
            &owner_event,
        )
        .map(|_assignment| ())?;
    let demotion = role_event("yard.management_role_set", yard_id, Some("admin"));
    let revocation = role_event("yard.management_role_revoked", yard_id, None);
    if repository.set_yard_management_role(
        yard_id,
        "user_fixture",
        YardManagementRole::Admin,
        102,
        &demotion,
    ) != Err(RepositoryError::Conflict)
        || repository.revoke_yard_management_role(yard_id, "user_fixture", 102, &revocation)
            != Err(RepositoryError::Conflict)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn role_event(action: &str, yard_id: &str, to: Option<&str>) -> NewAuditEvent {
    let mut event = super::yard_event(action, "yard_management_role", "yardId", yard_id, 102);
    event.metadata.extend([
        ("from".to_owned(), AuditValue::String("owner".to_owned())),
        (
            "userId".to_owned(),
            AuditValue::String("user_fixture".to_owned()),
        ),
    ]);
    if let Some(to) = to {
        event
            .metadata
            .push(("to".to_owned(), AuditValue::String(to.to_owned())));
    }
    event
}
