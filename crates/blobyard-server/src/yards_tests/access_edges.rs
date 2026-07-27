use super::{
    super::{access, presentation, require_manage},
    faulted_state, request,
};
use crate::{
    auth::Principal,
    contract_test_support::{assert_error, send},
    test_support::error_status,
    transfers::test_seams,
};
use axum::http::StatusCode;
use blobyard_api_client::{
    GetYardAccessQuery, GrantYardAccessPrincipalKind, GrantYardAccessRequest,
    RevokeYardAccessRequest, SetYardVisibilityRequest, YardVisibility,
};

pub(super) fn grant_request(yard_id: &str) -> GrantYardAccessRequest {
    GrantYardAccessRequest {
        yard_id: yard_id.to_owned(),
        principal_kind: GrantYardAccessPrincipalKind::User,
        principal_id: "user_reader".to_owned(),
        app_roles: Vec::new(),
        environment_id: None,
        expires_at: None,
    }
}

pub(super) fn started_yard_id(
    fixture: &test_seams::TransferFixture,
    principal: &Principal,
) -> String {
    let _ = super::super::deploy::start(
        &fixture.state,
        principal,
        &request("client-deploy-access-01"),
        Ok(1),
    )
    .expect("deploy start");
    fixture
        .state
        .repository
        .list_web_yards(&fixture.project.id)
        .expect("Yard list")
        .into_iter()
        .next()
        .expect("Yard")
        .id
}

pub(super) fn manager_fixture() -> (test_seams::TransferFixture, Principal, String) {
    let fixture = test_seams::fixture(&["yard:manage"]);
    seed_reader(&fixture);
    let principal = Principal(fixture.principal.clone());
    let yard_id = started_yard_id(&fixture, &principal);
    (fixture, principal, yard_id)
}

pub(super) fn seed_reader(fixture: &test_seams::TransferFixture) {
    let user = blobyard_testkit::local_user("workspace_fixture", "user_reader", None, 1);
    fixture
        .state
        .repository
        .create_local_user(
            &user,
            &blobyard_testkit::login_key("userkey_reader", &user.id, '8', 1),
            &blobyard_testkit::local_user_event("audit_user_reader", &user, "user.created", 1),
        )
        .expect("reader fixture");
}

#[test]
fn access_mutations_require_human_yard_managers() {
    let fixture = test_seams::fixture(&["yard:manage"]);
    let mut machine = Principal(fixture.principal.clone());
    machine.0.id = "machine_fixture".to_owned();
    assert_eq!(
        error_status(require_manage(&machine)),
        StatusCode::FORBIDDEN
    );
    let mut reader = Principal(fixture.principal);
    reader.0.scopes = vec!["yard:read".to_owned()];
    assert_eq!(error_status(require_manage(&reader)), StatusCode::FORBIDDEN);
}

#[test]
fn grant_validation_rejects_unbounded_principals_roles_and_expiries() {
    let (fixture, principal, yard_id) = manager_fixture();
    let invalid_principals = ["", " padded ", "line\nbreak"];
    for principal_id in invalid_principals {
        let mut invalid = grant_request(&yard_id);
        invalid.principal_id = principal_id.to_owned();
        assert_eq!(
            error_status(access::grant(&fixture.state, &principal, &invalid, Ok(1))),
            StatusCode::BAD_REQUEST,
            "principal {principal_id:?}"
        );
    }
    let mut oversized = grant_request(&yard_id);
    oversized.principal_id = "p".repeat(257);
    let mut crowded = grant_request(&yard_id);
    crowded.app_roles = (0..17).map(|index| format!("role{index}")).collect();
    assert_eq!(
        error_status(access::grant(&fixture.state, &principal, &crowded, Ok(1))),
        StatusCode::CONFLICT
    );
    let mut duplicated = grant_request(&yard_id);
    duplicated.app_roles = vec!["viewer".to_owned(), "viewer".to_owned()];
    let mut unbounded_role = grant_request(&yard_id);
    unbounded_role.app_roles = vec!["r".repeat(65)];
    let mut foreign_environment = grant_request(&yard_id);
    foreign_environment.environment_id = Some("yardenv_unknown".to_owned());
    let mut malformed_expiry = grant_request(&yard_id);
    malformed_expiry.expires_at = Some("soon".to_owned());
    let mut past_expiry = grant_request(&yard_id);
    past_expiry.expires_at = Some("1970-01-01T00:00:00Z".to_owned());
    let mut negative_expiry = grant_request(&yard_id);
    negative_expiry.expires_at = Some("1969-12-31T23:59:59Z".to_owned());
    for invalid in [
        oversized,
        duplicated,
        unbounded_role,
        foreign_environment,
        malformed_expiry,
        past_expiry,
        negative_expiry,
    ] {
        assert_eq!(
            error_status(access::grant(&fixture.state, &principal, &invalid, Ok(1))),
            StatusCode::BAD_REQUEST
        );
    }
}

#[test]
fn access_operations_propagate_repository_failures() {
    let (fixture, principal, yard_id) = manager_fixture();
    let query = GetYardAccessQuery {
        yard_id: yard_id.clone(),
    };
    for failure_index in 0..=2 {
        assert_eq!(
            error_status(access::get(
                &faulted_state(&fixture, failure_index),
                &principal,
                &query,
                Ok(1),
            )),
            StatusCode::INTERNAL_SERVER_ERROR,
            "read failure index {failure_index}"
        );
    }
    let visibility = SetYardVisibilityRequest {
        yard_id,
        visibility: YardVisibility::Owner,
    };
    for failure_index in 0..=2 {
        assert_eq!(
            error_status(access::set_visibility(
                &faulted_state(&fixture, failure_index),
                &principal,
                &visibility,
                Ok(1),
            )),
            StatusCode::INTERNAL_SERVER_ERROR,
            "visibility failure index {failure_index}"
        );
    }
}

#[test]
fn access_mutations_propagate_repository_failures() {
    let (fixture, principal, yard_id) = manager_fixture();
    let mut scoped = grant_request(&yard_id);
    scoped.environment_id = Some(format!("yardenv_{yard_id}"));
    for failure_index in 0..=2 {
        assert_eq!(
            error_status(access::grant(
                &faulted_state(&fixture, failure_index),
                &principal,
                &scoped,
                Ok(1),
            )),
            StatusCode::INTERNAL_SERVER_ERROR,
            "grant failure index {failure_index}"
        );
    }
    let revoke = RevokeYardAccessRequest {
        yard_id,
        grant_id: "grant_missing".to_owned(),
    };
    for failure_index in 0..=1 {
        assert_eq!(
            error_status(access::revoke(
                &faulted_state(&fixture, failure_index),
                &principal,
                &revoke,
                Ok(1),
            )),
            StatusCode::INTERNAL_SERVER_ERROR,
            "revoke failure index {failure_index}"
        );
    }
}

pub(super) fn grant_record() -> blobyard_contract::YardAccessGrantRecord {
    blobyard_contract::YardAccessGrantRecord {
        id: "yardgrant_edge".to_owned(),
        yard_id: "yard_edge".to_owned(),
        environment_id: None,
        principal_kind: blobyard_contract::YardAccessPrincipalKind::User,
        principal_id: "user_edge".to_owned(),
        app_roles: vec!["viewer".to_owned()],
        status: blobyard_contract::RevocableStatus::Active,
        created_at_ms: 1,
        created_by_principal: "fixture".to_owned(),
        expires_at_ms: None,
        revoked_at_ms: None,
    }
}

#[test]
fn grant_summaries_reject_unrepresentable_timestamps() {
    let mut record = grant_record();
    record.created_at_ms = u64::MAX;
    assert_eq!(
        error_status(presentation::grant_summary(record.clone())),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    record.created_at_ms = 1;
    record.expires_at_ms = Some(u64::MAX);
    assert_eq!(
        error_status(presentation::grant_summary(record)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn access_routes_map_extractor_rejections_to_the_public_error_contract() {
    let fixture = test_seams::fixture(&["yard:manage"]);
    assert_error(
        send(&fixture, "GET", "/v1/yards/access", b"", false).await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
    for path in [
        "/v1/yards/access/visibility",
        "/v1/yards/access/grant",
        "/v1/yards/access/revoke",
    ] {
        assert_error(
            send(&fixture, "POST", path, b"{", false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
    assert_error(
        send(
            &fixture,
            "POST",
            "/v1/yards/access/grant",
            br#"{"yardId":"yard_fixture","principalKind":"guest-invite","principalId":"ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","appRoles":[]}"#,
            false,
        )
        .await,
        StatusCode::BAD_REQUEST,
        "INVALID_REQUEST",
    )
    .await;
}
