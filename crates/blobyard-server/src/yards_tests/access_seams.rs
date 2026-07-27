use super::{
    super::{access, presentation},
    access_edge_tests::{grant_request, manager_fixture},
};
use crate::{error::ApiError, test_support::error_status};
use axum::http::StatusCode;
use blobyard_api_client::{
    GetYardAccessQuery, GrantYardAccessPrincipalKind as ApiGrantPrincipalKind,
    RevokeYardAccessRequest, SetYardVisibilityRequest, YardAccessPrincipalKind as ApiPrincipalKind,
    YardVisibility as ApiVisibility,
};
use blobyard_contract::{YardAccessPrincipalKind, YardVisibility};

#[test]
fn access_operations_propagate_clock_failures() {
    let (fixture, principal, yard_id) = manager_fixture();
    let query = GetYardAccessQuery {
        yard_id: yard_id.clone(),
    };
    let visibility = SetYardVisibilityRequest {
        yard_id: yard_id.clone(),
        visibility: ApiVisibility::Owner,
    };
    let revoke = RevokeYardAccessRequest {
        yard_id: yard_id.clone(),
        grant_id: "grant_missing".to_owned(),
    };
    let clock = || Err(ApiError::internal());
    assert_eq!(
        error_status(access::get(&fixture.state, &principal, &query, clock())),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(access::set_visibility(
            &fixture.state,
            &principal,
            &visibility,
            clock(),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(access::grant(
            &fixture.state,
            &principal,
            &grant_request(&yard_id),
            clock(),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(access::revoke(&fixture.state, &principal, &revoke, clock())),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn access_reads_conceal_foreign_workspaces_and_reject_corrupt_records() {
    let (fixture, principal, yard_id) = manager_fixture();
    let query = GetYardAccessQuery {
        yard_id: yard_id.clone(),
    };
    let mut foreign = principal.clone();
    foreign.0.workspace_id = "workspace_foreign".to_owned();
    assert_eq!(
        error_status(access::get(&fixture.state, &foreign, &query, Ok(1))),
        StatusCode::NOT_FOUND
    );
    let _ = access::grant(&fixture.state, &principal, &grant_request(&yard_id), Ok(1))
        .expect("granted access");
    fixture.corrupt_grant_timestamps();
    assert_eq!(
        error_status(access::get(&fixture.state, &principal, &query, Ok(1))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn grant_responses_reject_corrupt_persisted_records() {
    let (fixture, principal, yard_id) = manager_fixture();
    fixture.corrupt_future_grant_inserts();
    assert_eq!(
        error_status(access::grant(
            &fixture.state,
            &principal,
            &grant_request(&yard_id),
            Ok(1),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn access_presentation_maps_every_visibility_and_principal_kind() {
    for (domain, api) in [
        (YardVisibility::Public, ApiVisibility::Public),
        (YardVisibility::Owner, ApiVisibility::Owner),
        (YardVisibility::Selected, ApiVisibility::Selected),
        (YardVisibility::Workspace, ApiVisibility::Workspace),
        (
            YardVisibility::AuthenticatedLink,
            ApiVisibility::AuthenticatedLink,
        ),
        (
            YardVisibility::AnyAuthenticated,
            ApiVisibility::AnyAuthenticated,
        ),
    ] {
        assert_eq!(presentation::api_visibility(domain), api);
        assert_eq!(presentation::domain_visibility(api), domain);
    }
    for (domain, api) in [
        (YardAccessPrincipalKind::User, ApiPrincipalKind::User),
        (YardAccessPrincipalKind::Group, ApiPrincipalKind::Group),
        (
            YardAccessPrincipalKind::GuestInvite,
            ApiPrincipalKind::GuestInvite,
        ),
        (YardAccessPrincipalKind::Link, ApiPrincipalKind::Link),
    ] {
        let mut record = super::access_edge_tests::grant_record();
        record.principal_kind = domain;
        assert_eq!(
            presentation::grant_summary(record)
                .expect("grant summary")
                .principal_kind,
            api
        );
    }
    for (api, domain) in [
        (ApiGrantPrincipalKind::User, YardAccessPrincipalKind::User),
        (ApiGrantPrincipalKind::Group, YardAccessPrincipalKind::Group),
        (ApiGrantPrincipalKind::Link, YardAccessPrincipalKind::Link),
    ] {
        assert_eq!(presentation::domain_principal_kind(api), domain);
    }
}
