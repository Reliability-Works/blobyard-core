fn access_role_fixture() -> (
    crate::transfers::test_seams::TransferFixture,
    crate::auth::Principal,
    SetYardAccessRolesRequest,
) {
    let (fixture, principal, yard_id) = manager_fixture();
    let _ = access::grant(&fixture.state, &principal, &grant_request(&yard_id), Ok(1))
        .expect("access grant");
    let grant_id = fixture
        .state
        .repository
        .list_yard_access_grants(&yard_id, 2)
        .expect("access grants")[0]
        .id
        .clone();
    let request = SetYardAccessRolesRequest {
        yard_id,
        grant_id,
        app_roles: Vec::new(),
    };
    (fixture, principal, request)
}

#[test]
fn access_role_handler_covers_failure_seams() {
    let (fixture, principal, request) = access_role_fixture();
    assert_eq!(
        error_status(identity::set_access_roles(
            &fixture.state,
            &principal,
            &request,
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    for failure_index in 0..=2 {
        assert_eq!(
            error_status(identity::set_access_roles(
                &faulted_state(&fixture, failure_index),
                &principal,
                &request,
                Ok(2),
            )),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[test]
fn access_role_handler_covers_missing_grants_and_presentation_errors() {
    let (fixture, principal, request) = access_role_fixture();
    let missing = SetYardAccessRolesRequest {
        yard_id: request.yard_id.clone(),
        grant_id: "yardgrant_missing".to_owned(),
        app_roles: Vec::new(),
    };
    assert_eq!(
        error_status(identity::set_access_roles(
            &fixture.state,
            &principal,
            &missing,
            Ok(2),
        )),
        StatusCode::NOT_FOUND
    );
    let _ = identity::set_access_roles(
        &corrupting_state(&fixture, Corruption::CompletedVersion),
        &principal,
        &request,
        Ok(2),
    )
    .expect("uncorrupted access grant");
    assert_eq!(
        error_status(identity::set_access_roles(
            &corrupting_state(&fixture, Corruption::YardAccessGrantTimestamp),
            &principal,
            &request,
            Ok(2),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
