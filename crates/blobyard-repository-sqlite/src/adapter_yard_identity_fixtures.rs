use super::SqliteRepository;
use blobyard_contract::{YardIdentityRepository, YardManagementRole};

pub(in crate::adapter) fn approve_access_policy(
    repository: &SqliteRepository,
    yard_id: &str,
    owner_user_id: &str,
    at: u64,
) {
    assign_owner(repository, yard_id, owner_user_id, at);
    let digest = "a".repeat(64);
    let policy_event = blobyard_testkit::yard_policy_event(yard_id, &digest, at + 1);
    repository
        .set_yard_application_policy(
            yard_id,
            &digest,
            blobyard_testkit::yard_application_policy(),
            "fixture",
            at + 1,
            &policy_event,
        )
        .expect("application policy");
}

fn assign_owner(repository: &SqliteRepository, yard_id: &str, owner_user_id: &str, at: u64) {
    let owner_event = blobyard_testkit::yard_owner_event(yard_id, owner_user_id, at);
    repository
        .set_yard_management_role(
            yard_id,
            owner_user_id,
            YardManagementRole::Owner,
            at,
            &owner_event,
        )
        .expect("owner assignment");
}
