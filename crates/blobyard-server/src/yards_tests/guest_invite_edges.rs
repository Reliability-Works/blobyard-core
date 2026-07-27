#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    super::{deploy, guest_invites},
    access_edge_tests::manager_fixture,
    request,
};
use crate::test_support::error_status;
use axum::http::StatusCode;
use blobyard_api_client::{
    CreateYardGuestInviteRequest, ListYardGuestInvitesQuery, RevokeYardGuestInviteRequest,
};
use blobyard_core::Slug;

#[test]
fn guest_invite_management_rejects_zero_pages_and_maps_cursors() {
    let (fixture, principal, yard_id) = manager_fixture();
    assert_zero_page_rejected(&fixture.state, &principal, &yard_id);
    let invite_request = invitation_request(&yard_id, "guest@example.test");
    let _ = guest_invites::create(&fixture.state, &principal, &invite_request, Ok(1))
        .expect("invitation");
    assert_cursor_after_second_invitation(&fixture.state, &principal, &yard_id, &invite_request);
    let before = guest_authority_counts(&fixture.state, &yard_id);
    let mut invalid_origin = fixture.state;
    invalid_origin.public_origin = "not a URL".to_owned();
    let mut invalid_origin_invite = invite_request;
    invalid_origin_invite.email = "origin@example.test".to_owned();
    assert_eq!(
        error_status(guest_invites::create(
            &invalid_origin,
            &principal,
            &invalid_origin_invite,
            Ok(1),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(guest_authority_counts(&invalid_origin, &yard_id), before);
}

fn assert_zero_page_rejected(
    state: &crate::api::AppState,
    principal: &crate::auth::Principal,
    yard_id: &str,
) {
    assert_eq!(
        error_status(guest_invites::list(
            state,
            principal,
            &ListYardGuestInvitesQuery {
                yard_id: yard_id.to_owned(),
                cursor: None,
                limit: Some(0),
            },
        )),
        StatusCode::BAD_REQUEST
    );
}

fn assert_cursor_after_second_invitation(
    state: &crate::api::AppState,
    principal: &crate::auth::Principal,
    yard_id: &str,
    invite_request: &CreateYardGuestInviteRequest,
) {
    let mut second_invite = invite_request.clone();
    second_invite.email = "second@example.test".to_owned();
    let _ =
        guest_invites::create(state, principal, &second_invite, Ok(1)).expect("second invitation");
    let page = guest_invites::list(
        state,
        principal,
        &ListYardGuestInvitesQuery {
            yard_id: yard_id.to_owned(),
            cursor: None,
            limit: Some(1),
        },
    )
    .expect("first page");
    let page_json = serde_json::to_value(page.0).expect("serialized page");
    assert!(page_json["data"]["nextCursor"].is_string());
}

fn guest_authority_counts(state: &crate::api::AppState, yard_id: &str) -> (usize, usize, usize) {
    (
        state
            .repository
            .list_yard_guest_invites(yard_id, None, 50)
            .expect("guest invitations")
            .items
            .len(),
        state
            .repository
            .list_yard_access_grants(yard_id, 1)
            .expect("guest grants")
            .len(),
        created_audit_count(state),
    )
}

#[test]
fn guest_invitation_revocation_conceals_foreign_yards() {
    let (fixture, principal, yard_id) = manager_fixture();
    let invite_request = invitation_request(&yard_id, "guest@example.test");
    let _ = guest_invites::create(&fixture.state, &principal, &invite_request, Ok(1))
        .expect("invitation");
    let invitation_id = fixture
        .state
        .repository
        .list_yard_guest_invites(&yard_id, None, 50)
        .expect("guest invitations")
        .items
        .into_iter()
        .next()
        .expect("guest invitation")
        .id;

    let mut second = request("client-deploy-guest-other");
    second.name = Slug::new("other").expect("yard name");
    let _ = deploy::start(&fixture.state, &principal, &second, Ok(2)).expect("second yard");
    let other_yard_id = fixture
        .state
        .repository
        .list_web_yards(&fixture.project.id)
        .expect("yards")
        .into_iter()
        .find(|yard| yard.id != yard_id)
        .expect("other yard")
        .id;
    assert_eq!(
        error_status(guest_invites::revoke(
            &fixture.state,
            &principal,
            &RevokeYardGuestInviteRequest {
                yard_id: other_yard_id,
                invitation_id,
            },
            Ok(3),
        )),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn guest_invitation_defaults_expiry_and_conceals_foreign_environment_scope() {
    let (fixture, principal, yard_id) = manager_fixture();
    let mut defaulted = invitation_request(&yard_id, "default@example.test");
    defaulted.expires_at = None;
    let created = guest_invites::create(&fixture.state, &principal, &defaulted, Ok(100))
        .expect("defaulted invitation");
    let created_json = serde_json::to_value(created.0).expect("created invitation");
    assert_eq!(
        created_json["data"]["invitation"]["expiresAt"],
        crate::transfer_grants::format_expiry(
            100 + blobyard_contract::YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS,
        )
        .expect("default expiry")
    );

    let mut ambiguous = invitation_request(&yard_id, "a@b@c");
    ambiguous.expires_at = None;
    assert_eq!(
        error_status(guest_invites::create(
            &fixture.state,
            &principal,
            &ambiguous,
            Ok(100),
        )),
        StatusCode::BAD_REQUEST
    );

    let mut second = request("client-deploy-guest-environment");
    second.name = Slug::new("environment-owner").expect("yard name");
    let _ = deploy::start(&fixture.state, &principal, &second, Ok(101)).expect("second yard");
    let foreign_yard = fixture
        .state
        .repository
        .list_web_yards(&fixture.project.id)
        .expect("yards")
        .into_iter()
        .find(|yard| yard.id != yard_id)
        .expect("foreign yard");
    let foreign_environment = fixture
        .state
        .repository
        .list_yard_environments(&foreign_yard.id)
        .expect("foreign environments")
        .into_iter()
        .next()
        .expect("foreign environment");
    let mut foreign_scope = invitation_request(&yard_id, "foreign@example.test");
    foreign_scope.environment_id = Some(foreign_environment.id);
    foreign_scope.expires_at = None;
    assert_eq!(
        error_status(guest_invites::create(
            &fixture.state,
            &principal,
            &foreign_scope,
            Ok(102),
        )),
        StatusCode::NOT_FOUND
    );
}

pub(super) fn invitation_request(yard_id: &str, email: &str) -> CreateYardGuestInviteRequest {
    CreateYardGuestInviteRequest {
        yard_id: yard_id.to_owned(),
        environment_id: None,
        email: email.to_owned(),
        app_roles: Vec::new(),
        expires_at: Some(
            crate::transfer_grants::format_expiry(
                1 + blobyard_contract::YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS,
            )
            .expect("expiry"),
        ),
    }
}

fn created_audit_count(state: &crate::api::AppState) -> usize {
    state
        .repository
        .list_audit(&state.default_workspace.id, None, 100)
        .expect("audit page")
        .items
        .into_iter()
        .filter(|event| event.action == "yard.guest_invite.created")
        .count()
}
