use super::super::identity;
use crate::{
    Repository, api::AppState, repository_fault_tests::Corruption,
    repository_fault_tests::FaultingRepository, transfers::test_seams::TransferFixture,
};
use blobyard_api_client::{
    SetYardApplicationPolicyRequest, SetYardManagementRoleRequest, YardManagementRole as ApiRole,
};
use blobyard_core::{ApplicationPolicyGraph, ApplicationRoleDefinition};
use std::{collections::BTreeMap, sync::Arc};

pub(super) fn corrupting_state(fixture: &TransferFixture, corruption: Corruption) -> AppState {
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let mut state = fixture.state.clone();
    state.repository = Arc::new(FaultingRepository::corrupting(inner, corruption));
    state
}

pub(super) fn role_request(yard_id: &str, user_id: &str) -> SetYardManagementRoleRequest {
    SetYardManagementRoleRequest {
        yard_id: yard_id.to_owned(),
        user_id: user_id.to_owned(),
        role: ApiRole::Owner,
    }
}

pub(super) fn policy_request(yard_id: &str) -> SetYardApplicationPolicyRequest {
    let viewer = ApplicationRoleDefinition {
        inherits: Vec::new(),
        permissions: vec!["content.read".to_owned()],
    };
    let policy = ApplicationPolicyGraph {
        default_role: Some("viewer".to_owned()),
        roles: BTreeMap::from([("viewer".to_owned(), viewer)]),
    };
    SetYardApplicationPolicyRequest {
        yard_id: yard_id.to_owned(),
        source_manifest_digest: "a".repeat(64),
        policy,
    }
}

pub(super) fn policy_manager_fixture() -> (TransferFixture, crate::auth::Principal, String) {
    let (fixture, principal, yard_id) = super::access_edge_tests::manager_fixture();
    let request = role_request(&yard_id, "user_reader");
    let _ = identity::set_management_role(&fixture.state, &principal, &request, Ok(1))
        .expect("owner role");
    (fixture, principal, yard_id)
}

pub(super) fn seed_policy(
    fixture: &TransferFixture,
    principal: &crate::auth::Principal,
    request: &SetYardApplicationPolicyRequest,
) {
    let _ = identity::set_application_policy(&fixture.state, principal, request, Ok(2))
        .expect("application policy");
}
