#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::tests::{Fixture, INVITATION_ID, KEY_HASH, TOKEN_HASH, grant, invitation};
use blobyard_contract::{
    RepositoryError, WebYardRepository, YardGuestInviteCursor, YardGuestRepository,
};
use blobyard_testkit::{granted_event, revoked_event};

use super::tests::events::event;

#[test]
fn guest_public_lookup_and_mutation_inputs_fail_closed() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.repository.list_yard_guest_invites("", None, 1),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.yard_guest_invite_by_id(""),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .repository
            .pending_yard_guest_invite_by_token("bad", 1),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .repository
            .pending_yard_guest_invite_by_token(TOKEN_HASH, u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.authenticate_yard_guest_key("bad", 1),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .repository
            .authenticate_yard_guest_key(KEY_HASH, u64::MAX),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.revoke_yard_guest_invite(
            "",
            INVITATION_ID,
            2,
            &event("revoked", &invitation(), None, 2),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.revoke_yard_guest_invite(
            "yard_guest",
            "",
            2,
            &event("revoked", &invitation(), None, 2),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.revoke_yard_guest_invite(
            "yard_guest",
            "ygi_ffffffffffffffffffffffffffffffff",
            2,
            &event("revoked", &invitation(), None, 2),
        ),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn guest_list_cursor_inputs_fail_closed() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture
            .repository
            .list_yard_guest_invites("yard_guest", None, 0),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .repository
            .list_yard_guest_invites("yard_guest", None, 51),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.list_yard_guest_invites(
            "yard_guest",
            Some(&YardGuestInviteCursor {
                created_at_ms: u64::MAX,
                id: INVITATION_ID.to_owned(),
            }),
            1,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture.repository.list_yard_guest_invites(
            "yard_guest",
            Some(&YardGuestInviteCursor {
                created_at_ms: 1,
                id: String::new(),
            }),
            1,
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn guest_list_returns_an_exact_next_cursor() {
    let fixture = Fixture::new();
    fixture.create();
    let mut second = invitation();
    second.id = "ygi_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_owned();
    second.email = "second@example.test".to_owned();
    second.token_hash = "e".repeat(64);
    second.grant_id = "yardgrant_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_owned();
    second.created_at_ms = 2;
    second.expires_at_ms += 1;
    let mut second_grant = grant();
    second_grant.id.clone_from(&second.grant_id);
    second_grant.principal_id.clone_from(&second.id);
    second_grant.created_at_ms = second.created_at_ms;
    second_grant.expires_at_ms = Some(second.expires_at_ms);
    fixture
        .repository
        .create_yard_guest_invite(
            &second,
            &second_grant,
            &event("created", &second, None, second.created_at_ms),
        )
        .expect("second invitation");

    let page = fixture
        .repository
        .list_yard_guest_invites("yard_guest", None, 1)
        .expect("first page");
    assert_eq!(
        page.items,
        vec![
            fixture
                .repository
                .yard_guest_invite_by_id(&second.id)
                .expect("second")
        ]
    );
    assert!(page.next_cursor.is_some());
}

#[test]
fn guest_events_and_foreign_revocation_fail_closed() {
    let fixture = Fixture::new();
    let created_invitation = invitation();
    let mut invalid_event = event("created", &created_invitation, None, 1);
    invalid_event.action = "wrong.action".to_owned();
    assert_eq!(
        fixture
            .repository
            .create_yard_guest_invite(&created_invitation, &grant(), &invalid_event),
        Err(RepositoryError::InvalidInput)
    );
    fixture.create();
    assert_eq!(
        fixture.repository.revoke_yard_guest_invite(
            "yard_other",
            INVITATION_ID,
            2,
            &event("revoked", &created_invitation, None, 2),
        ),
        Err(RepositoryError::NotFound)
    );
    let mut invalid_revoke = event("revoked", &created_invitation, None, 2);
    invalid_revoke.action = "wrong.action".to_owned();
    assert_eq!(
        fixture.repository.revoke_yard_guest_invite(
            "yard_guest",
            INVITATION_ID,
            2,
            &invalid_revoke,
        ),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn generic_grant_mutations_reject_guest_invitation_authority() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture
            .repository
            .insert_yard_access_grant(&grant(), &granted_event("yard_guest", &grant(), 1),),
        Err(RepositoryError::InvalidInput)
    );
    fixture.create();
    assert_eq!(
        fixture.repository.revoke_yard_access_grant(
            "yard_guest",
            "yardgrant_guest",
            2,
            &revoked_event("yard_guest", "yardgrant_guest", 2),
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        fixture
            .repository
            .pending_yard_guest_invite_by_token(TOKEN_HASH, 2)
            .expect("dedicated authority remains pending")
            .status,
        blobyard_contract::YardGuestInviteStatus::Pending
    );
}

#[test]
fn guest_invitation_capacity_is_enforced() {
    let capacity = Fixture::new();
    capacity
        .repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "WITH RECURSIVE seq(n) AS (
               VALUES(1) UNION ALL SELECT n + 1 FROM seq WHERE n < 100
             )
             INSERT INTO yard_access_grants
               (id, yard_id, environment_id, principal_kind, principal_id, app_roles,
                status, created_at_ms, created_by_principal, expires_at_ms, revoked_at_ms)
             SELECT printf('yardgrant_capacity_%d', n), 'yard_guest', NULL, 'guest-invite',
                    printf('ygi_%032x', n), '[]', 'active', 1, 'operator', 600001, NULL
             FROM seq;
             WITH RECURSIVE seq(n) AS (
               VALUES(1) UNION ALL SELECT n + 1 FROM seq WHERE n < 100
             )
             INSERT INTO yard_guest_invitations
               (id, workspace_id, project_id, yard_id, environment_id, email, token_hash,
                status, accepted_subject_id, grant_id, created_at_ms, expires_at_ms,
                accepted_at_ms, revoked_at_ms)
             SELECT printf('ygi_%032x', n), 'workspace_guest', 'project_guest', 'yard_guest',
                    NULL, printf('capacity%d@example.test', n), printf('%064x', n), 'pending',
                    NULL, printf('yardgrant_capacity_%d', n), 1, 600001, NULL, NULL
             FROM seq;",
        )
        .expect("capacity");
    assert_eq!(
        capacity.repository.create_yard_guest_invite(
            &invitation(),
            &grant(),
            &event("created", &invitation(), None, 1),
        ),
        Err(RepositoryError::Conflict)
    );
}
