#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    super::guest_invites, access_edge_tests::manager_fixture, faulted_state,
    guest_invite_edge_tests::invitation_request, identity_handler_support::corrupting_state,
};
use crate::{
    contract_test_support::send, repository_fault_tests::Corruption, test_support::error_status,
    transfers::test_seams,
};
use axum::http::StatusCode;
use blobyard_api_client::{ListYardGuestInvitesQuery, RevokeYardGuestInviteRequest};
use blobyard_contract::YardGuestInviteRecord;
use blobyard_core::GeneratedSecretKind;

#[tokio::test]
async fn guest_invite_handlers_reject_missing_authority_and_malformed_inputs() {
    let reader = test_seams::fixture(&["yard:read"]);
    for (method, path, body) in [
        ("GET", "/v1/yards/guest-invites?yardId=yard", &b""[..]),
        ("POST", "/v1/yards/guest-invites", &b"{}"[..]),
        ("POST", "/v1/yards/guest-invites/revoke", &b"{}"[..]),
    ] {
        assert_eq!(
            send(&reader, method, path, body, false).await.status(),
            StatusCode::FORBIDDEN
        );
    }

    let (manager, _principal, _yard_id) = manager_fixture();
    assert_eq!(
        send(
            &manager,
            "GET",
            "/v1/yards/guest-invites?yardId=yard&limit=bad",
            b"",
            false,
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    for path in ["/v1/yards/guest-invites", "/v1/yards/guest-invites/revoke"] {
        assert_eq!(
            send(&manager, "POST", path, b"{", false).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
}

#[test]
fn guest_invite_list_propagates_yard_cursor_repository_and_presentation_failures() {
    let (fixture, principal, yard_id) = manager_fixture();
    let request = invitation_request(&yard_id, "list@example.test");
    let _ = guest_invites::create(&fixture.state, &principal, &request, Ok(1)).expect("invitation");

    assert_eq!(
        error_status(guest_invites::list(
            &faulted_state(&fixture, 0),
            &principal,
            &list_query(&yard_id, None),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(guest_invites::list(
            &fixture.state,
            &principal,
            &list_query(&yard_id, Some("not-a-cursor")),
        )),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        error_status(guest_invites::list(
            &faulted_state(&fixture, 1),
            &principal,
            &list_query(&yard_id, None),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let corrupted = corrupting_state(&fixture, Corruption::YardGuestInviteTimestamp);
    assert_eq!(
        error_status(guest_invites::list(
            &corrupted,
            &principal,
            &list_query(&yard_id, None),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn guest_invite_creation_propagates_clock_expiry_repository_and_presentation_failures() {
    assert_guest_invite_clock_and_expiry_failures();
    assert_guest_invite_repository_and_presentation_failures();
}

fn assert_guest_invite_clock_and_expiry_failures() {
    let (fixture, principal, yard_id) = manager_fixture();
    let request = invitation_request(&yard_id, "create@example.test");
    assert_eq!(
        error_status(guest_invites::create(
            &fixture.state,
            &principal,
            &request,
            Err(crate::error::ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let mut malformed_expiry = request.clone();
    malformed_expiry.expires_at = Some("not-a-time".to_owned());
    assert_eq!(
        error_status(guest_invites::create(
            &fixture.state,
            &principal,
            &malformed_expiry,
            Ok(1),
        )),
        StatusCode::BAD_REQUEST
    );
    let mut overflow = request;
    overflow.expires_at = None;
    assert_eq!(
        error_status(guest_invites::create(
            &fixture.state,
            &principal,
            &overflow,
            Ok(u64::MAX),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

fn assert_guest_invite_repository_and_presentation_failures() {
    let (fixture, principal, yard_id) = manager_fixture();
    let request = invitation_request(&yard_id, "create@example.test");
    assert_eq!(
        error_status(guest_invites::create(
            &faulted_state(&fixture, 1),
            &principal,
            &request,
            Ok(1),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let corrupted = corrupting_state(&fixture, Corruption::YardGuestInviteTimestamp);
    assert_eq!(
        error_status(guest_invites::create(
            &corrupted,
            &principal,
            &request,
            Ok(1),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let token = crate::auth::generate_token(GeneratedSecretKind::YardGuestInvitation);
    assert_eq!(
        error_status(guest_invites::invitation_url(
            &fixture.state,
            "valid-host",
            &token,
            2,
            2,
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn guest_invite_revocation_propagates_each_repository_and_presentation_failure() {
    for (failure_index, expected) in [
        (0, StatusCode::INTERNAL_SERVER_ERROR),
        (1, StatusCode::INTERNAL_SERVER_ERROR),
        (2, StatusCode::INTERNAL_SERVER_ERROR),
    ] {
        let (fixture, principal, yard_id) = manager_fixture();
        let invitation_id = create_invitation(&fixture.state, &principal, &yard_id);
        let state = if failure_index == 0 {
            fixture.state.clone()
        } else {
            faulted_state(&fixture, failure_index)
        };
        let now = if failure_index == 0 {
            Err(crate::error::ApiError::internal())
        } else {
            Ok(2)
        };
        assert_eq!(
            error_status(guest_invites::revoke(
                &state,
                &principal,
                &RevokeYardGuestInviteRequest {
                    yard_id,
                    invitation_id,
                },
                now,
            )),
            expected
        );
    }

    let (fixture, principal, yard_id) = manager_fixture();
    let invitation_id = create_invitation(&fixture.state, &principal, &yard_id);
    let corrupted = corrupting_state(&fixture, Corruption::YardGuestInviteTimestamp);
    assert_eq!(
        error_status(guest_invites::revoke(
            &corrupted,
            &principal,
            &RevokeYardGuestInviteRequest {
                yard_id,
                invitation_id,
            },
            Ok(2),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn guest_invite_presentation_rejects_each_unrepresentable_timestamp() {
    let (fixture, principal, yard_id) = manager_fixture();
    let invitation_id = create_invitation(&fixture.state, &principal, &yard_id);
    let record = fixture
        .state
        .repository
        .yard_guest_invite_by_id(&invitation_id)
        .expect("invitation");
    for corrupted in timestamp_corruptions(record) {
        assert_eq!(
            error_status(guest_invites::present(corrupted)),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

fn list_query(yard_id: &str, cursor: Option<&str>) -> ListYardGuestInvitesQuery {
    ListYardGuestInvitesQuery {
        yard_id: yard_id.to_owned(),
        cursor: cursor.map(ToOwned::to_owned),
        limit: None,
    }
}

fn create_invitation(
    state: &crate::api::AppState,
    principal: &crate::auth::Principal,
    yard_id: &str,
) -> String {
    let request = invitation_request(yard_id, "revoke@example.test");
    let response = guest_invites::create(state, principal, &request, Ok(1)).expect("invitation");
    serde_json::to_value(response.0).expect("response")["data"]["invitation"]["id"]
        .as_str()
        .expect("invitation ID")
        .to_owned()
}

fn timestamp_corruptions(record: YardGuestInviteRecord) -> Vec<YardGuestInviteRecord> {
    let mut accepted = record.clone();
    accepted.accepted_at_ms = Some(u64::MAX);
    let mut created = record.clone();
    created.created_at_ms = u64::MAX;
    let mut expires = record.clone();
    expires.expires_at_ms = u64::MAX;
    let mut revoked = record;
    revoked.revoked_at_ms = Some(u64::MAX);
    vec![accepted, created, expires, revoked]
}
