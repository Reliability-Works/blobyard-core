#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::tests::{
    EXPIRES_AT_MS, Fixture, INVITATION_ID, SUBJECT_ID, TOKEN_HASH, invitation, key, subject,
};
use blobyard_contract::{RepositoryError, YardGuestRepository, YardSubjectKind, YardSubjectRecord};
use blobyard_testkit::sqlite_guest_yard_continuation;

use super::tests::events::event;

#[test]
fn guest_acceptance_validation_and_corrupt_rows_fail_closed() {
    let fixture = Fixture::new();
    fixture.create();
    let invitation = invitation();

    let mut invalid_accept = event("accepted", &invitation, Some(SUBJECT_ID), 2);
    invalid_accept.action = "wrong.action".to_owned();
    assert_eq!(
        fixture.repository.accept_yard_guest_invite(
            TOKEN_HASH,
            &super::tests::subject(),
            &key(),
            &sqlite_guest_yard_continuation(),
            &invalid_accept,
            2,
        ),
        Err(RepositoryError::InvalidInput)
    );

    let member = member_subject(None);
    assert_eq!(
        super::super::yard_guest_rows::validate_subject(&member),
        Ok(())
    );
    assert_eq!(
        fixture.repository.accept_yard_guest_invite(
            TOKEN_HASH,
            &member,
            &key(),
            &sqlite_guest_yard_continuation(),
            &event("accepted", &invitation, Some(&member.id), 2),
            2,
        ),
        Err(RepositoryError::InvalidInput)
    );
    assert_eq!(
        super::super::yard_guest_rows::validate_subject(&member_subject(Some(INVITATION_ID))),
        Err(RepositoryError::InvalidInput)
    );
    let mut invalid_key = key();
    invalid_key.expires_at_ms = invalid_key.created_at_ms;
    assert_eq!(
        super::super::yard_guest_keys::validate_new(&invalid_key),
        Err(RepositoryError::InvalidInput)
    );

    fixture
        .repository
        .test_connection()
        .expect("connection")
        .execute(
            "UPDATE yard_access_grants SET principal_id = 'foreign' WHERE id = 'yardgrant_guest'",
            [],
        )
        .expect("corrupt grant");
    assert_eq!(
        fixture
            .repository
            .pending_yard_guest_invite_by_token(TOKEN_HASH, 2),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn guest_acceptance_validates_time_subject_and_key_through_the_atomic_path() {
    let fixture = Fixture::new();
    fixture.create();
    assert_eq!(
        fixture.repository.accept_yard_guest_invite(
            TOKEN_HASH,
            &subject(),
            &key(),
            &sqlite_guest_yard_continuation(),
            &event("accepted", &invitation(), Some(SUBJECT_ID), u64::MAX),
            u64::MAX,
        ),
        Err(RepositoryError::InvalidInput)
    );

    let mut invalid_subject = subject();
    invalid_subject.id.clear();
    assert_eq!(
        fixture.repository.accept_yard_guest_invite(
            TOKEN_HASH,
            &invalid_subject,
            &key(),
            &sqlite_guest_yard_continuation(),
            &event("accepted", &invitation(), Some(&invalid_subject.id), 2),
            2,
        ),
        Err(RepositoryError::InvalidInput)
    );

    let mut invalid_key = key();
    invalid_key.secret_hash = "bad".to_owned();
    assert_eq!(
        fixture.repository.accept_yard_guest_invite(
            TOKEN_HASH,
            &subject(),
            &invalid_key,
            &sqlite_guest_yard_continuation(),
            &event("accepted", &invitation(), Some(SUBJECT_ID), 2),
            2,
        ),
        Err(RepositoryError::InvalidInput)
    );

    let mut invalid_key_text = key();
    invalid_key_text.id.clear();
    assert_eq!(
        super::super::yard_guest_keys::validate_new(&invalid_key_text),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn guest_mutations_fail_closed_when_expected_rows_disappear() {
    let ignored_accept = Fixture::new();
    ignored_accept.create();
    ignored_accept
        .repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER ignore_guest_accept
             BEFORE UPDATE ON yard_guest_invitations
             WHEN NEW.status = 'accepted'
             BEGIN SELECT RAISE(IGNORE); END;",
        )
        .expect("accept trigger");
    assert_eq!(accept(&ignored_accept), Err(RepositoryError::Conflict));

    let ignored_revoke = Fixture::new();
    ignored_revoke.create();
    ignored_revoke
        .repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER ignore_guest_revoke
             BEFORE UPDATE ON yard_guest_invitations
             WHEN NEW.status = 'revoked'
             BEGIN SELECT RAISE(IGNORE); END;",
        )
        .expect("revoke trigger");
    assert_eq!(revoke(&ignored_revoke), Err(RepositoryError::Conflict));

    let ignored_grant = Fixture::new();
    ignored_grant.create();
    ignored_grant
        .repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER ignore_guest_grant_revoke
             BEFORE UPDATE ON yard_access_grants
             WHEN NEW.status = 'revoked'
             BEGIN SELECT RAISE(IGNORE); END;",
        )
        .expect("grant trigger");
    assert_eq!(revoke(&ignored_grant), Err(RepositoryError::Conflict));
}

fn accept(fixture: &Fixture) -> Result<(), RepositoryError> {
    fixture
        .repository
        .accept_yard_guest_invite(
            TOKEN_HASH,
            &subject(),
            &key(),
            &sqlite_guest_yard_continuation(),
            &event("accepted", &invitation(), Some(SUBJECT_ID), 2),
            2,
        )
        .map(|_accepted| ())
}

fn revoke(fixture: &Fixture) -> Result<(), RepositoryError> {
    fixture
        .repository
        .revoke_yard_guest_invite(
            "yard_guest",
            INVITATION_ID,
            2,
            &event("revoked", &invitation(), None, 2),
        )
        .map(|_revoked| ())
}

fn member_subject(invitation_id: Option<&str>) -> YardSubjectRecord {
    YardSubjectRecord {
        id: "member_fixture".to_owned(),
        kind: YardSubjectKind::Member,
        workspace_id: "workspace_guest".to_owned(),
        local_user_id: Some("member_fixture".to_owned()),
        invitation_id: invitation_id.map(ToOwned::to_owned),
        created_at_ms: 2,
        revoked_at_ms: None,
    }
}

#[test]
fn guest_fixture_expiry_remains_in_sqlite_range() {
    assert!(EXPIRES_AT_MS <= i64::MAX.cast_unsigned());
}
