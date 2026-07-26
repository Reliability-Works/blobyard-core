use super::YardConformanceRepository;
use blobyard_contract::{RepositoryError, YardManagementRole};

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
        .map(|_assignment| ())
}
