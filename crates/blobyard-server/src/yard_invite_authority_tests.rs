#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::invitation as accept_invitation;
use crate::{test_support::error_status, transfers::test_seams};
use axum::http::StatusCode;
use blobyard_contract::{YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS, YardGuestInviteStatus};
use blobyard_core::SecretString;

#[test]
fn invalid_exchange_origin_does_not_consume_invitation_authority() {
    let fixture = test_seams::fixture(&["yard:read"]);
    let started = super::super::test_support::start_yard(&fixture.state);
    let raw_token = format!("bygi_{}", "a".repeat(64));
    let expires_at_ms = 1 + YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS;
    let invitation = super::super::test_support::create_invitation(
        &fixture.state,
        &started.yard,
        &raw_token,
        expires_at_ms,
    );
    let continuation = crate::yard_session_contracts::issue_invitation(
        &fixture.state.yard_continuation_key,
        &started.yard.host_label,
        "/",
        1,
        expires_at_ms,
    )
    .expect("continuation");
    let claims = crate::yard_session_contracts::verify_invitation(
        &fixture.state.yard_continuation_key,
        &continuation,
        2,
    )
    .expect("claims");
    let token = SecretString::new(raw_token).expect("token");
    let grant_count = fixture
        .state
        .repository
        .list_yard_access_grants(&started.yard.id, 2)
        .expect("grants before invalid origin")
        .len();
    let accepted_audit_count = audit_count(&fixture.state, "yard.guest_invite.accepted");
    let mut invalid_origin = fixture.state;
    invalid_origin.web_yard_origin = "not a URL".to_owned();

    assert_eq!(
        error_status(accept_invitation(
            &invalid_origin,
            &token,
            &continuation,
            &claims,
            &invitation,
            2,
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_invitation_authority_unchanged(
        &invalid_origin,
        &started.yard.id,
        &token,
        grant_count,
        accepted_audit_count,
    );
}

fn assert_invitation_authority_unchanged(
    state: &crate::api::AppState,
    yard_id: &str,
    token: &SecretString,
    grant_count: usize,
    accepted_audit_count: usize,
) {
    assert_eq!(
        state
            .repository
            .pending_yard_guest_invite_by_token(&crate::auth::hash(token.expose_secret()), 2)
            .expect("invitation remains pending")
            .status,
        YardGuestInviteStatus::Pending
    );
    assert_eq!(
        state
            .repository
            .list_yard_access_grants(yard_id, 2)
            .expect("grants after invalid origin")
            .len(),
        grant_count
    );
    assert_eq!(
        audit_count(state, "yard.guest_invite.accepted"),
        accepted_audit_count
    );
}

fn audit_count(state: &crate::api::AppState, action: &str) -> usize {
    state
        .repository
        .list_audit(&state.default_workspace.id, None, 100)
        .expect("audit page")
        .items
        .into_iter()
        .filter(|event| event.action == action)
        .count()
}
