#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    super::{access, identity},
    access_edge_tests::{grant_request, manager_fixture},
    faulted_state,
    identity_handler_support::{
        corrupting_state, policy_manager_fixture, policy_request, role_request, seed_policy,
    },
};
use crate::{error::ApiError, repository_fault_tests::Corruption, test_support::error_status};
use axum::http::StatusCode;
use blobyard_api_client::{
    GetYardApplicationPolicyQuery, ListYardManagementRolesQuery, RevokeYardManagementRoleRequest,
    SetYardAccessRolesRequest,
};

#[test]
fn management_role_listing_covers_failures_corruption_and_pagination() {
    let (fixture, principal, yard_id) = manager_fixture();
    let query = ListYardManagementRolesQuery {
        yard_id: yard_id.clone(),
        cursor: None,
    };
    for failure_index in 0..=1 {
        assert_eq!(
            error_status(identity::list_management_roles(
                &faulted_state(&fixture, failure_index),
                &principal,
                &query,
            )),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
    let mut invalid_cursor = query.clone();
    invalid_cursor.cursor = Some("not-a-cursor".to_owned());
    assert_eq!(
        error_status(identity::list_management_roles(
            &fixture.state,
            &principal,
            &invalid_cursor,
        )),
        StatusCode::BAD_REQUEST
    );

    let request = role_request(&yard_id, "user_reader");
    let _ = identity::set_management_role(&fixture.state, &principal, &request, Ok(2))
        .expect("management role");
    assert_eq!(
        error_status(identity::list_management_roles(
            &corrupting_state(&fixture, Corruption::YardManagementRoleTimestamp),
            &principal,
            &query,
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let _ = fixture.seed_yard_management_role_page(&yard_id);
    let page =
        identity::list_management_roles(&fixture.state, &principal, &query).expect("role page");
    assert!(
        serde_json::to_value(page.0).expect("role page JSON")["data"]["nextCursor"].is_string()
    );
}

#[test]
fn management_role_set_covers_every_failure_seam() {
    let (fixture, principal, yard_id) = manager_fixture();
    let set = role_request(&yard_id, "user_reader");
    assert_eq!(
        error_status(identity::set_management_role(
            &fixture.state,
            &principal,
            &set,
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    for failure_index in 0..=2 {
        assert_eq!(
            error_status(identity::set_management_role(
                &faulted_state(&fixture, failure_index),
                &principal,
                &set,
                Ok(2),
            )),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
    assert_eq!(
        error_status(identity::set_management_role(
            &corrupting_state(&fixture, Corruption::YardManagementRoleTimestamp),
            &principal,
            &set,
            Ok(2),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn management_role_revoke_covers_every_failure_seam() {
    let (fixture, principal, yard_id) = manager_fixture();
    let set = role_request(&yard_id, "user_reader");
    let _ = identity::set_management_role(&fixture.state, &principal, &set, Ok(3))
        .expect("management role");
    let revoke = RevokeYardManagementRoleRequest {
        yard_id: yard_id.clone(),
        user_id: "user_reader".to_owned(),
    };
    assert_eq!(
        error_status(identity::revoke_management_role(
            &fixture.state,
            &principal,
            &revoke,
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    for failure_index in 0..=2 {
        assert_eq!(
            error_status(identity::revoke_management_role(
                &faulted_state(&fixture, failure_index),
                &principal,
                &revoke,
                Ok(4),
            )),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
    let missing = RevokeYardManagementRoleRequest {
        yard_id,
        user_id: "user_missing".to_owned(),
    };
    assert_eq!(
        error_status(identity::revoke_management_role(
            &fixture.state,
            &principal,
            &missing,
            Ok(4),
        )),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn application_policy_get_covers_failures_and_corruption() {
    let (fixture, principal, yard_id) = policy_manager_fixture();
    let query = GetYardApplicationPolicyQuery {
        yard_id: yard_id.clone(),
    };
    for failure_index in 0..=1 {
        assert_eq!(
            error_status(identity::get_application_policy(
                &faulted_state(&fixture, failure_index),
                &principal,
                &query,
            )),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
    let request = policy_request(&yard_id);
    seed_policy(&fixture, &principal, &request);
    let _ = identity::get_application_policy(
        &corrupting_state(&fixture, Corruption::CompletedVersion),
        &principal,
        &query,
    )
    .expect("uncorrupted application policy");
    assert_eq!(
        error_status(identity::get_application_policy(
            &corrupting_state(&fixture, Corruption::YardPolicyTimestamp),
            &principal,
            &query,
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn application_policy_set_covers_failures_and_validation() {
    let (fixture, principal, yard_id) = policy_manager_fixture();
    let mut request = policy_request(&yard_id);
    assert_eq!(
        error_status(identity::set_application_policy(
            &fixture.state,
            &principal,
            &request,
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    for failure_index in 0..=2 {
        assert_eq!(
            error_status(identity::set_application_policy(
                &faulted_state(&fixture, failure_index),
                &principal,
                &request,
                Ok(2),
            )),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
    request.policy.default_role = Some("missing".to_owned());
    assert_eq!(
        error_status(identity::set_application_policy(
            &fixture.state,
            &principal,
            &request,
            Ok(2),
        )),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn application_policy_set_covers_corrupt_revisions_and_timestamps() {
    let (fixture, principal, yard_id) = policy_manager_fixture();
    let request = policy_request(&yard_id);
    seed_policy(&fixture, &principal, &request);
    assert_eq!(
        error_status(identity::set_application_policy(
            &corrupting_state(&fixture, Corruption::YardPolicyRevision),
            &principal,
            &request,
            Ok(3),
        )),
        StatusCode::CONFLICT
    );
    assert_eq!(
        error_status(identity::set_application_policy(
            &corrupting_state(&fixture, Corruption::YardPolicyTimestamp),
            &principal,
            &request,
            Ok(3),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

include!("identity_access_role_handler_edges.rs");
